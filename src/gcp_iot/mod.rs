use async_trait::async_trait;
use color_eyre::Result;
use google_cloud_iot_jwt::create_google_jwt_es256;
use log::info;
pub use paho_mqtt::AsyncClient;
use paho_mqtt::{
    ConnectOptions, ConnectOptionsBuilder, CreateOptionsBuilder, Properties, ReasonCode,
    SslOptions, SslOptionsBuilder, SslVersion, MQTT_VERSION_3_1_1,
};
use std::env;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs;

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
        .keep_alive_interval(Duration::from_secs(60 * 20))
        .user_name("ignore")
        .clean_session(true)
        .password(jwt)
        .ssl_options(ssl_ops)
        .finalize()
}
#[async_trait]
pub trait GoogleIotConnect {
    async fn gcp_connect() -> Result<AsyncClient>;
}

#[async_trait]
impl GoogleIotConnect for AsyncClient {
    async fn gcp_connect() -> Result<AsyncClient> {
        // create the MqttJWT object
        let jwt = new_password_jwt().await;

        let project_id =
            env::var("PROJECT_ID").expect("Missing PROJECT_ID in environment variables");
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
}

#[cfg(test)]
mod test {
    use super::*;
    use color_eyre::Result;
    use dotenv::dotenv;
    use paho_mqtt::{Message, QOS_1};

    #[tokio::test]
    async fn push_to_custom_topics() -> Result<()> {
        dotenv().ok();
        color_eyre::install()?;
        let client = AsyncClient::gcp_connect().await?;

        let device_id = env::var("DEVICE_ID").expect("Missing DEVICE_ID in environment variables");

        client
            .subscribe(format!("/devices/{device_id}/config"), QOS_1)
            .await?;

        let msg = Message::new(
            format!("/devices/{device_id}/events/piston"),
            "piston debug data",
            QOS_1,
        );
        client.publish(msg).await?;

        let msg = Message::new(
            format!("/devices/{device_id}/events/robot"),
            "robot debug data",
            QOS_1,
        );
        client.publish(msg).await?;

        let msg = Message::new(
            format!("/devices/{device_id}/events/feeder"),
            "feeder debug data",
            QOS_1,
        );
        client.publish(msg).await?;

        Ok(())
    }
}
