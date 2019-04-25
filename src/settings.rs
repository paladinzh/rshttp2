#[derive(Debug, Clone)]
pub enum SettingKey {
    HeaderTableSize,
    EnablePush,
    MaxConcurrentStreams,
    InitialWindowSize,
    MaxFrameSize,
    MaxHeaderListSize,
}

pub const ALL_SETTING_KEYS: [SettingKey; 6] = [
    SettingKey::HeaderTableSize,
    SettingKey::EnablePush,
    SettingKey::MaxConcurrentStreams,
    SettingKey::InitialWindowSize,
    SettingKey::MaxFrameSize,
    SettingKey::MaxHeaderListSize,
];

impl SettingKey {
    pub fn from_h2_id(id: usize) -> SettingKey {
        assert!(id >= 1 && id <= ALL_SETTING_KEYS.len(), "id={}", id);
        ALL_SETTING_KEYS[id - 1].clone()
    }
}

#[derive(Debug)]
pub struct Settings {
    ack: bool,
    values: [u32; 7],
}

impl Settings {
    pub fn new() -> Settings {
        Settings{
            ack: false,
            values: [
                0, // placeholder,
                4096, // SETTINGS_HEADER_TABLE_SIZE
                1, // SETTINGS_ENABLE_PUSH
                100, // SETTINGS_MAX_CONCURRENT_STREAMS. RFC-7540 does not specify a default value. nghttp2 engages 100 as default.
                65535, // SETTINGS_INITIAL_WINDOW_SIZE
                16384, // SETTINGS_MAX_FRAME_SIZE
                u32::max_value(), // SETTINGS_MAX_HEADER_LIST_SIZE. By RFC-7540, it should be unlimited.
            ]
        }
    }

    pub fn get(&self, key: SettingKey) -> u32 {
        match key {
            SettingKey::HeaderTableSize => self.values[1],
            SettingKey::EnablePush => self.values[2],
            SettingKey::MaxConcurrentStreams => self.values[3],
            SettingKey::InitialWindowSize => self.values[4],
            SettingKey::MaxFrameSize => self.values[5],
            SettingKey::MaxHeaderListSize => self.values[6],
        }
    }

    pub fn set(&mut self, key: SettingKey, value: u32) {
        match key {
            SettingKey::HeaderTableSize => self.values[1] = value,
            SettingKey::EnablePush => self.values[2] = value,
            SettingKey::MaxConcurrentStreams => self.values[3] = value,
            SettingKey::InitialWindowSize => self.values[4] = value,
            SettingKey::MaxFrameSize => self.values[5] = value,
            SettingKey::MaxHeaderListSize => self.values[6] = value,
        }
    }

    pub fn set_ack(&mut self) {
        self.ack = true;
    }

    pub fn clear_ack(&mut self) {
        self.ack = false;
    }
}
