use color_eyre::Result;
use dotenv::dotenv;
use futures::stream::StreamExt;
use google_cloud_iot_jwt::create_google_jwt_es256;
use gpio_cdev::{Chip, EventRequestFlags, LineRequestFlags};
use paho_mqtt::{
    AsyncClient, ConnectOptions, ConnectOptionsBuilder, CreateOptionsBuilder, Message, Properties,
    ReasonCode, SslOptions, SslOptionsBuilder, SslVersion, MQTT_VERSION_3_1_1, QOS_1,
};
use std::env;
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::sync::mpsc::{channel, unbounded_channel};
extern crate pretty_env_logger;
#[macro_use]
extern crate log;
use serde::{Deserialize, Serialize};

async fn new_password_jwt() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    let private_key = fs::read_to_string("ec_private.pem")
        .await
        .expect("No private key found");

    create_google_jwt_es256(
        "digital-twin-experiment",
        &private_key,
        now.as_secs().try_into().unwrap(),
    )
    .unwrap()
    .to_string()
}

fn get_ssl_ops() -> SslOptions {
    let pub_key =
        env::var("CA_CERTIFICATE").expect("Missing CA_CERTIFICATE in environment variables");
    let pri_key = env::var("PRIVATE_KEY").expect("Missing PRIVATE_KEY in environment variable");

    SslOptionsBuilder::new()
        .trust_store(pub_key)
        .unwrap()
        .ssl_version(SslVersion::Tls_1_2)
        .private_key(pri_key)
        .unwrap()
        .finalize()
}

fn get_connect_ops(ssl_ops: SslOptions, jwt: impl Into<String>) -> ConnectOptions {
    ConnectOptionsBuilder::new()
        .mqtt_version(MQTT_VERSION_3_1_1)
        .keep_alive_interval(StdDuration::from_secs(60 * 20))
        .user_name("ignore")
        .clean_session(true)
        .password(jwt)
        .ssl_options(ssl_ops)
        .finalize()
}

async fn get_client() -> Result<AsyncClient> {
    // create the MqttJWT object
    let jwt = new_password_jwt().await;

    let project_id = env::var("PROJECT_ID").expect("Missing PROJECT_ID in environment variables");
    let device_id = env::var("DEVICE_ID").expect("Missing DEVICE_ID in environment variables");
    let registry_id =
        env::var("REGISTRY_ID").expect("Missing REGISTRY_ID in environment variables");
    let region = env::var("REGION").expect("Missing REGION in environment variables");
    let mqtt_client_id = format!(
        "projects/{project_id}/locations/{region}/registries/{registry_id}/devices/{device_id}"
    );

    let ssl_ops = get_ssl_ops();
    let connect_ops = get_connect_ops(ssl_ops, jwt);

    let create_options = CreateOptionsBuilder::new()
        .server_uri("ssl://mqtt.googleapis.com:8883")
        .client_id(mqtt_client_id)
        .finalize();

    let mut client = AsyncClient::new(create_options).unwrap();

    // Google IoT will automatically discount after 20 minutes of inactivity, unfortunately, the we
    // need to update the password to reconnect
    client.set_disconnected_callback(
        |client: &AsyncClient, _properties: Properties, reason_code: ReasonCode| {
            match reason_code {
                ReasonCode::KeepAliveTimeout => {
                    // the callback function cannot be async, so we have to block
                    let handle = tokio::runtime::Handle::current();
                    let jwt = handle.block_on(new_password_jwt());

                    let ssl_options = get_ssl_ops();
                    let connect_options = get_connect_ops(ssl_options, jwt);
                    client.connect(connect_options);
                }
                _ => {
                    info!("Disconnected for reasons other than time out, unable to resolve")
                }
            }
        },
    );

    client.connect(connect_ops).await?;
    Ok(client)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    pretty_env_logger::init();
    color_eyre::install()?;

    let (mut tx, mut rx) = unbounded_channel::<String>();

    let mut client = get_client().await?;
    let mut stream = client.get_stream(100);

    let device_id = env::var("DEVICE_ID").expect("Missing DEVICE_ID in environment variables");

    client
        .subscribe(format!("/devices/{device_id}/config"), QOS_1)
        .await?;

    tokio::task::spawn(async move {
        while let Some(msg) = stream.next().await {
            info!("{:?}", msg)
        }
    });

    let mut gpio_chip = Chip::new("/dev/gpiochip0")
        .expect("Unable to gain access to /dev/gpiochip0, make sure you have rw permission to it");

    let material_line: u32 = env::var("MATERIAL_LINE")
        .expect("Missing MATERIAL_LINE in environment variables")
        .parse()
        .expect("MATERIAL_LINE cannot be parsed as unsigned integer");

    let material_line = gpio_chip
        .get_line(material_line)
        .expect(&format!("Cannot gain access to line {}", material_line));

    // todo: make sure it's actually the rising edge
    let mut material_stream = material_line.async_events(
        LineRequestFlags::INPUT,
        EventRequestFlags::RISING_EDGE,
        "material event consumer",
    )?;

    let pos_1: u32 = env::var("POS_1")
        .expect("POS_1 is not defined")
        .parse()
        .expect("POS_1 cannot be parsed as unsigned int");

    let pos_1 = gpio_chip.get_line(pos_1)?;
    let mut pos_1_stream = pos_1.async_events(
        LineRequestFlags::INPUT,
        EventRequestFlags::RISING_EDGE,
        "pos 1 consumer",
    )?;

    let pos_15: u32 = env::var("POS_15")
        .expect("POS_15 is not defined")
        .parse()
        .expect("POS_15 cannot be parsed as unsigned int");

    let pos_15 = gpio_chip.get_line(pos_15)?;
    let pos_15_stream = pos_15.async_events(
        LineRequestFlags::INPUT,
        EventRequestFlags::RISING_EDGE,
        "pos 15 consumer",
    )?;

    let loc_reached: u32 = env::var("LOC_REACHED")
        .expect("LOC_REACHED not defined")
        .parse()
        .expect("LOC_REACHED cannot be parsed as unsigned int");

    let loc_reached = gpio_chip.get_line(loc_reached)?;
    let mut loc_reached_stream = loc_reached.async_events(
        LineRequestFlags::INPUT,
        EventRequestFlags::RISING_EDGE,
        "loc reached consumer",
    )?;

    // todo: what about the piston?

    loop {
        let mut pos = TrackPositions::Position1;
        // wait for track to move to position 1
        pos_1_stream.next().await;
        info!("Track moving to position 1");
        let msg = Message::new(
            format!("/devices/{device_id}/events"),
            serde_json::to_string(&pos)?,
            QOS_1,
        );

        // todo: perhaps just kick start the process but don't await?, use a channel to sent the
        // message to be processed by another task
        client.publish(msg).await?;
        loc_reached_stream.next().await;
        info!("Position 1 reached");

        pos = TrackPositions::Position15;
        let msg = Message::new(
            format!("/devices/{device_id}/events"),
            serde_json::to_string(&pos)?,
            QOS_1,
        );
        client.publish(msg).await?;
    }

    Ok(())
}

#[derive(Serialize, Deserialize)]
enum TrackPositions {
    Position1,
    // From1To15,
    Position15,
    // From15To1,
}
