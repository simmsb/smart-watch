use std::sync::{Arc, Mutex};

use bus::Bus;
use esp_idf_svc::espnow::EspNowClient;

use crate::message::Message;

pub struct EspNowData(EspNowClient);

fn handle_msg(mac: &[u8], msg: &[u8], bus: &Mutex<Bus<Message>>) {
    tracing::info!(?mac, "Got message");

    match postcard::from_bytes::<Message>(msg) {
        Ok(msg) => {
            bus.lock().unwrap().broadcast(msg);
        }
        Err(err) => tracing::error!(%err, "Failed to decode message"),
    }
}

pub fn espnow_setup(bus: Arc<Mutex<Bus<Message>>>) -> color_eyre::Result<EspNowData> {
    let client = EspNowClient::new()?;

    client.register_recv_cb(move |mac, msg| handle_msg(mac, msg, &bus))?;

    Ok(EspNowData(client))
}
