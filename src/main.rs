mod gcp_iot;
mod manufacturing_components;
mod utils;

use crate::gcp_iot::message::StartRequest;
use crate::gcp_iot::GoogleIotConnect;
use crate::manufacturing_components::feeder::{Event as FeederEvent, Feeder};
use crate::manufacturing_components::program::{ManufacturingProgram, SimplifiedScenario2};
use base64::{decode, URL_SAFE};
use color_eyre::Result;
use dotenv::dotenv;
use futures::stream::StreamExt;
use gpio_cdev::Chip;
use log::{info, log};
use paho_mqtt::{AsyncClient, QOS_1};
use pretty_env_logger;
use std::env;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    pretty_env_logger::init();
    color_eyre::install()?;

    // any events we wish to sent to the google cloud is sent across the channel to be processed by a
    // dedicated task
    let (mut tx, mut rx) = unbounded_channel();

    // a dedicated task just to process events to be sent to google cloud
    let event_processor = tokio::task::spawn(async move {
        while let Some(event) = rx.recv().await {
            println!("{event:?}");
        }
    });

    let mut client = AsyncClient::gcp_connect().await?;
    let mut msg_stream = client.get_stream(100);

    let device_id = env::var("DEVICE_ID").expect("Missing DEVICE_ID in environment variables");

    // config used to ease development, feel free to change to any more appropriate topic names
    let config_topic = format!("/devices/{device_id}/config");
    client.subscribe(&config_topic, QOS_1).await?;

    let mut gpio_chip = Chip::new("/dev/gpiochip0")
        .expect("Unable to gain access to /dev/gpiochip0, make sure you have read and write permission to it");

    let material_line: u32 = env::var("MATERIAL_LINE")
        .expect("Missing MATERIAL_LINE in environment variables")
        .parse()
        .expect("MATERIAL_LINE cannot be parsed as unsigned integer");

    let program_controller: u32 = env::var("PROGRAM_CONTROL")
        .expect("Missing PROGRAM_CONTROL in environment variables")
        .parse()
        .expect("PROGRAM_CONTROL cannot be parsed as unsigned integer");

    let mut program_controller = SimplifiedScenario2::new(&mut gpio_chip, program_controller)?;

    let mut material_feeder = Feeder::new("Material feeder", 10, &mut gpio_chip, material_line)?;

    let gcp_listener = tokio::task::spawn(async move {
        while let Some(msg) = msg_stream.next().await {
            let msg = msg.unwrap();

            if msg.topic() == &config_topic {
                // this is inefficient, only there to easy development
                let payload_str = msg.payload_str();
                println!("{payload_str:?}");

                let request: StartRequest = serde_json::from_str(payload_str.as_ref()).unwrap();

                // unwrap for ease of development
                simplified_scenario2_cycle(
                    request.count,
                    &mut material_feeder,
                    &mut program_controller,
                    &mut tx,
                )
                .await
                .unwrap();
            }
        }
    });

    gcp_listener.await?;
    event_processor.await?;
    Ok(())
}

/// Start running the simplified scenario 2 program until there are no materials left, returning the
/// the number of materials picked up
async fn simplified_scenario2_cycle(
    count: u32,
    feeder: &mut Feeder,
    program: &mut SimplifiedScenario2,
    tx: &mut UnboundedSender<FeederEvent>,
) -> Result<u32> {
    program.start()?;

    for _i in 0..count {
        assert!(!feeder.is_empty());
        // wait for some material to be picked up and sent the event across the channel
        let event = feeder.async_next_event().await?;

        // tx should be alive, unwrap is safe
        tx.send(event).unwrap();

        // wait for the materials to be pushed
        feeder.async_next_event().await?;
    }

    program.stop()?;

    Ok(count)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio::{join, time};

    async fn input_loop(
        tx: mpsc::UnboundedSender<String>,
        repeat: u32,
        fake_wait: Duration,
    ) -> Result<()> {
        // time duration to fake waiting for GPIO inputs
        // number chosen to be relatively small
        let fake_dur = fake_wait;

        for _ in 0..repeat {
            // simulate waiting for the track to signal to go to position 1
            time::sleep(fake_dur).await;
            tx.send("Robot wants go to position 1".to_string())?;

            // simulate reaching position 1
            time::sleep(fake_dur).await;
            tx.send("Position 1 is reached".to_string())?;

            // simulate the material getting picked up
            time::sleep(fake_dur).await;
            tx.send("Material is picked up".to_string())?;

            // simulate the track wanting to go to track 15
            time::sleep(fake_dur).await;
            tx.send("Robot wants to go to position 15".to_string())?;

            // simulate the track reaching position
            time::sleep(fake_dur).await;
            tx.send("Position 15 reached".to_string())?;

            // simulate the track wanted to position 66
            time::sleep(fake_dur).await;
            tx.send("Robot wants to go position 66".to_string())?;

            // simulate the track reaching position 66
            time::sleep(fake_dur).await;
            tx.send("Position 66 reached".to_string())?;

            // simulate the piston starting to depress
            time::sleep(fake_dur).await;
            tx.send("Piston depressed".to_string())?;

            // simulate the piston back to steady
            time::sleep(fake_dur).await;
            tx.send("Piston back to steady".to_string())?;
        }
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn manufacturing_event_loop() {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        let producer_handle = tokio::spawn(input_loop(tx, 64, Duration::from_millis(50)));

        let consumer_handle = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                println!("{msg}");
            }
        });

        let (_res1, _res2) = join!(producer_handle, consumer_handle);
    }
}
