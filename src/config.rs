use gtk::gio::prelude::*;
use gtk::{gio, glib};
use serde_json::{json, Value};

type ConfigError = Box<dyn std::error::Error>;

pub fn read_config(file: &gio::File) -> Result<(Value, Option<glib::GString>), ConfigError> {
    match file.load_contents(gio::Cancellable::NONE) {
        Ok((bytes, tag)) => {
            let config: Value = serde_json::from_slice(&bytes)?;
            if !config.is_object() {
                return Err("Palobar configuration must be a JSON object".into());
            }
            Ok((config, tag))
        }
        Err(error) if error.matches(gio::IOErrorEnum::NotFound) => Ok((json!({}), None)),
        Err(error) => Err(error.into()),
    }
}

pub fn update_config(
    file: &gio::File,
    update: impl Fn(&mut Value) -> Result<bool, ConfigError>,
) -> Result<(), ConfigError> {
    for _ in 0..3 {
        let (mut config, tag) = read_config(file)?;
        if !update(&mut config)? {
            return Ok(());
        }
        let bytes = serde_json::to_vec_pretty(&config)?;
        match file.replace_contents(
            &bytes,
            tag.as_deref(),
            false,
            gio::FileCreateFlags::NONE,
            gio::Cancellable::NONE,
        ) {
            Ok(_) => return Ok(()),
            Err(error) if error.matches(gio::IOErrorEnum::WrongEtag) => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err("Palobar configuration changed repeatedly while saving".into())
}
