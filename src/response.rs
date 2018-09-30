#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcobeeStatus {
    target_heating_cooling_state: u8,
    target_temperature: f32,
    target_relative_humidity: f32,
    current_heating_cooling_state: u8,
    current_temperature: f32,
    current_relative_humidity: f32,
}

impl EcobeeStatus {
    pub fn new(
        mode: u8,
        target: f32,
        current: f32,
        humidity: f32,
        target_humidity: f32,
    ) -> EcobeeStatus {
        EcobeeStatus {
            target_heating_cooling_state: mode,
            target_temperature: target,
            target_relative_humidity: target_humidity,
            current_heating_cooling_state: mode,
            current_temperature: current,
            current_relative_humidity: humidity,
        }
    }
}

pub enum EcobeeResponse {
    Status(EcobeeStatus),
}
