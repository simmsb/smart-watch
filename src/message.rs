use std::sync::Mutex;

use bus::{Bus, BusReader};
use color_eyre::eyre::eyre;
use once_cell::sync::Lazy;

include!(concat!(env!("OUT_DIR"), "/messages.items.rs"));

static BUS: Lazy<Mutex<Bus<Message>>> = Lazy::new(|| Mutex::new(Bus::new(10)));

pub fn validate_msg(msg: Message) -> color_eyre::Result<Message> {
    const SECURITY_BY_OBSCURITY: u32 = 3387062;

    if msg.origin != SECURITY_BY_OBSCURITY {
        return Err(eyre!("Bad origin, should be {}", SECURITY_BY_OBSCURITY));
    }

    Ok(msg)
}

pub fn push_message(msg: Message) -> color_eyre::Result<()> {
    let mut bus = BUS.lock().unwrap();

    bus.try_broadcast(msg)
        .map_err(|m| eyre!("Failed to broadcast message {:?}", m))?;

    Ok(())
}

pub fn get_receiver() -> BusReader<Message> {
    BUS.lock().unwrap().add_rx()
}
