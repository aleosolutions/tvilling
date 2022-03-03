mod gcp_iot;
mod manufacturing_components;
mod utils;
use crate::gcp_iot::GoogleIotConnect;
use color_eyre::Result;
use dotenv::dotenv;
use futures::stream::StreamExt;
use gpio_cdev::{Chip, EventRequestFlags, LineRequestFlags};
use log::info;
use paho_mqtt::{AsyncClient, QOS_1};
use pretty_env_logger;
use std::env;
use tokio::sync::mpsc::unbounded_channel;

#[allow(unused_variables, unused_mut)]
#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    pretty_env_logger::init();
    color_eyre::install()?;

    let (mut tx, mut rx) = unbounded_channel::<String>();

    let mut client = AsyncClient::gcp_connect().await?;
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

    // loop {
    //     let mut pos = TrackPositions::Position1;
    //     // wait for track to move to position 1
    //     pos_1_stream.next().await;
    //     info!("Track moving to position 1");
    //     let msg = Message::new(
    //         format!("/devices/{device_id}/events"),
    //         serde_json::to_string(&pos)?,
    //         QOS_1,
    //     );
    //
    //     // todo: perhaps just kick start the process but don't await?, use a channel to sent the
    //     // message to be processed by another task
    //     client.publish(msg).await?;
    //     loc_reached_stream.next().await;
    //     info!("Position 1 reached");
    //
    //     pos = TrackPositions::Position15;
    //     let msg = Message::new(
    //         format!("/devices/{device_id}/events"),
    //         serde_json::to_string(&pos)?,
    //         QOS_1,
    //     );
    //     client.publish(msg).await?;
    // }

    Ok(())
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
