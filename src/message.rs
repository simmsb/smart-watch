use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub enum Message {
    PushDisplayMessage(String)
}
