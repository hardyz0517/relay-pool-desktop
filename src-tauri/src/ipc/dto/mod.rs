pub(crate) mod settings;
pub(crate) mod stations;

pub use settings::SettingsDto;
pub use stations::StationDto;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
pub struct TypeDescriptor {
    pub name: &'static str,
    pub typescript: &'static str,
}

#[cfg_attr(not(test), allow(dead_code))]
pub const REGISTERED_TYPES: &[TypeDescriptor] = &[settings::SETTINGS_TYPE, stations::STATION_TYPE];
