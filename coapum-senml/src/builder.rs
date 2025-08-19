//! Builder pattern for creating SenML packs

use crate::{SenMLPack, SenMLRecord, SenMLValue};

/// Builder for creating SenML packs with a fluent API
#[derive(Debug, Default)]
pub struct SenMLBuilder {
    base_name: Option<String>,
    base_time: Option<f64>, 
    base_unit: Option<String>,
    base_value: Option<f64>,
    base_sum: Option<f64>,
    records: Vec<SenMLRecord>,
}

impl SenMLBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the base name for all records
    pub fn base_name<S: Into<String>>(mut self, name: S) -> Self {
        self.base_name = Some(name.into());
        self
    }

    /// Set the base time for all records
    pub fn base_time(mut self, time: f64) -> Self {
        self.base_time = Some(time);
        self
    }

    /// Set the base unit for all records
    pub fn base_unit<S: Into<String>>(mut self, unit: S) -> Self {
        self.base_unit = Some(unit.into());
        self
    }

    /// Set the base value to add to all numeric values
    pub fn base_value(mut self, value: f64) -> Self {
        self.base_value = Some(value);
        self
    }

    /// Set the base sum value
    pub fn base_sum(mut self, sum: f64) -> Self {
        self.base_sum = Some(sum);
        self
    }

    /// Add a record with a numeric value
    pub fn add_value<S: Into<String>>(mut self, name: S, value: f64) -> Self {
        self.records.push(SenMLRecord::with_value(name, value));
        self
    }

    /// Add a record with a string value
    pub fn add_string_value<S: Into<String>, V: Into<String>>(mut self, name: S, value: V) -> Self {
        self.records.push(SenMLRecord::with_string_value(name, value));
        self
    }

    /// Add a record with a boolean value
    pub fn add_bool_value<S: Into<String>>(mut self, name: S, value: bool) -> Self {
        self.records.push(SenMLRecord::with_bool_value(name, value));
        self
    }

    /// Add a record with binary data
    pub fn add_data_value<S: Into<String>>(mut self, name: S, data: Vec<u8>) -> Self {
        self.records.push(SenMLRecord::with_data_value(name, data));
        self
    }

    /// Add a measurement with timestamp
    pub fn add_measurement<S: Into<String>>(mut self, name: S, value: f64, time: f64) -> Self {
        self.records.push(
            SenMLRecord::with_value(name, value).with_time(time)
        );
        self
    }

    /// Add a measurement with unit and timestamp
    pub fn add_measurement_with_unit<S: Into<String>, U: Into<String>>(
        mut self, 
        name: S, 
        value: f64, 
        unit: U, 
        time: f64
    ) -> Self {
        self.records.push(
            SenMLRecord::with_value(name, value)
                .with_unit(unit)
                .with_time(time)
        );
        self
    }

    /// Add a sum measurement
    pub fn add_sum<S: Into<String>>(mut self, name: S, sum: f64, time: f64) -> Self {
        self.records.push(
            SenMLRecord::new()
                .with_name(name)
                .with_sum(sum)  
                .with_time(time)
        );
        self
    }

    /// Add an existing record
    pub fn add_record(mut self, record: SenMLRecord) -> Self {
        self.records.push(record);
        self
    }

    /// Add multiple records at once
    pub fn add_records<I>(mut self, records: I) -> Self 
    where
        I: IntoIterator<Item = SenMLRecord>,
    {
        self.records.extend(records);
        self
    }

    /// Build the SenML pack
    pub fn build(self) -> SenMLPack {
        let mut records = Vec::new();

        // Create base record if we have base values
        if self.has_base_values() {
            let mut base_record = SenMLRecord::new();
            
            if let Some(bn) = self.base_name {
                base_record.n = Some(bn);
            }
            if let Some(bt) = self.base_time {
                base_record.t = Some(bt);
            }
            if let Some(bu) = self.base_unit {
                base_record.u = Some(bu);
            }
            if let Some(bv) = self.base_value {
                base_record.v = Some(bv);
            }
            if let Some(bs) = self.base_sum {
                base_record.s = Some(bs);
            }
            
            records.push(base_record);
        }

        // Add all the measurement records
        records.extend(self.records);

        SenMLPack { records }
    }

    /// Check if we have any base values set
    fn has_base_values(&self) -> bool {
        self.base_name.is_some() 
            || self.base_time.is_some()
            || self.base_unit.is_some()
            || self.base_value.is_some()
            || self.base_sum.is_some()
    }
}

/// Extensions for SenMLRecord to support builder pattern
impl SenMLRecord {
    /// Set the name of this record
    pub fn with_name<S: Into<String>>(mut self, name: S) -> Self {
        self.n = Some(name.into());
        self
    }

    /// Set update time for this record
    pub fn with_update_time(mut self, ut: f64) -> Self {
        self.ut = Some(ut);
        self
    }
}

/// Specialized builder for time-series data
#[derive(Debug)]
pub struct TimeSeriesBuilder {
    base_name: String,
    base_time: f64,
    base_unit: Option<String>,
    measurements: Vec<(f64, f64)>, // (relative_time, value)
}

impl TimeSeriesBuilder {
    /// Create a new time series builder
    pub fn new<S: Into<String>>(base_name: S, base_time: f64) -> Self {
        Self {
            base_name: base_name.into(),
            base_time,
            base_unit: None,
            measurements: Vec::new(),
        }
    }

    /// Set the unit for all measurements
    pub fn unit<S: Into<String>>(mut self, unit: S) -> Self {
        self.base_unit = Some(unit.into());
        self
    }

    /// Add a measurement at a relative time
    pub fn measurement(mut self, relative_time: f64, value: f64) -> Self {
        self.measurements.push((relative_time, value));
        self
    }

    /// Add measurements from an iterator
    pub fn measurements<I>(mut self, measurements: I) -> Self
    where
        I: IntoIterator<Item = (f64, f64)>,
    {
        self.measurements.extend(measurements);
        self
    }

    /// Add measurement with current timestamp
    pub fn measurement_now(mut self, value: f64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let relative_time = now - self.base_time;
        self.measurements.push((relative_time, value));
        self
    }

    /// Build the time series pack
    pub fn build(self) -> SenMLPack {
        let mut builder = SenMLBuilder::new()
            .base_name(&self.base_name)
            .base_time(self.base_time);

        if let Some(unit) = self.base_unit {
            builder = builder.base_unit(unit);
        }

        for (time, value) in self.measurements {
            builder = builder.add_measurement("", value, time);
        }

        builder.build()
    }
}

/// Specialized builder for device configuration
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    device_name: Option<String>,
    parameters: Vec<(String, SenMLValue)>,
}

impl ConfigBuilder {
    /// Create a new configuration builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the device name
    pub fn device<S: Into<String>>(mut self, name: S) -> Self {
        self.device_name = Some(name.into());
        self
    }

    /// Add a numeric parameter
    pub fn param_number<S: Into<String>>(mut self, name: S, value: f64) -> Self {
        self.parameters.push((name.into(), SenMLValue::Number(value)));
        self
    }

    /// Add a string parameter
    pub fn param_string<S: Into<String>, V: Into<String>>(mut self, name: S, value: V) -> Self {
        self.parameters.push((name.into(), SenMLValue::String(value.into())));
        self
    }

    /// Add a boolean parameter
    pub fn param_bool<S: Into<String>>(mut self, name: S, value: bool) -> Self {
        self.parameters.push((name.into(), SenMLValue::Boolean(value)));
        self
    }

    /// Build the configuration pack
    pub fn build(self) -> SenMLPack {
        let mut builder = SenMLBuilder::new();

        if let Some(device) = self.device_name {
            builder = builder.base_name(device);
        }

        for (name, value) in self.parameters {
            let record = match value {
                SenMLValue::Number(n) => SenMLRecord::with_value(name, n),
                SenMLValue::String(s) => SenMLRecord::with_string_value(name, s),
                SenMLValue::Boolean(b) => SenMLRecord::with_bool_value(name, b),
                SenMLValue::Data(d) => SenMLRecord::with_data_value(name, d),
            };
            builder = builder.add_record(record);
        }

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_builder() {
        let pack = SenMLBuilder::new()
            .base_name("device1/")
            .base_unit("Cel")
            .add_value("temp", 22.5)
            .add_value("humidity", 45.0)
            .build();

        assert_eq!(pack.records.len(), 3); // Base record + 2 measurements
        
        let base = &pack.records[0];
        assert_eq!(base.n, Some("device1/".to_string()));
        assert_eq!(base.u, Some("Cel".to_string()));
    }

    #[test]
    fn test_measurement_builder() {
        let pack = SenMLBuilder::new()
            .add_measurement_with_unit("temperature", 25.0, "Cel", 1640995200.0)
            .build();

        assert_eq!(pack.records.len(), 1);
        let record = &pack.records[0];
        assert_eq!(record.v, Some(25.0));
        assert_eq!(record.u, Some("Cel".to_string()));
        assert_eq!(record.t, Some(1640995200.0));
    }

    #[test]
    fn test_time_series_builder() {
        let base_time = 1640995200.0;
        let pack = TimeSeriesBuilder::new("sensor1/temp", base_time)
            .unit("Cel")
            .measurement(0.0, 22.0)
            .measurement(60.0, 22.5)
            .measurement(120.0, 23.0)
            .build();

        assert!(pack.records.len() >= 3);
        
        // Check base values are set
        let base = &pack.records[0];
        assert_eq!(base.n, Some("sensor1/temp".to_string()));
        assert_eq!(base.t, Some(base_time));
        assert_eq!(base.u, Some("Cel".to_string()));
    }

    #[test]
    fn test_config_builder() {
        let pack = ConfigBuilder::new()
            .device("device1/config/")
            .param_number("threshold", 25.0)
            .param_string("mode", "auto")
            .param_bool("enabled", true)
            .build();

        assert_eq!(pack.records.len(), 4); // Base + 3 params
        
        let base = &pack.records[0];
        assert_eq!(base.n, Some("device1/config/".to_string()));
    }

    #[test]
    fn test_mixed_values() {
        let pack = SenMLBuilder::new()
            .add_value("temp", 25.0)
            .add_string_value("status", "OK")
            .add_bool_value("enabled", true)
            .build();

        assert_eq!(pack.records.len(), 3);
        assert!(pack.records[0].v.is_some());
        assert!(pack.records[1].vs.is_some());
        assert!(pack.records[2].vb.is_some());
    }

    #[test]
    fn test_builder_with_no_base_values() {
        let pack = SenMLBuilder::new()
            .add_value("standalone", 42.0)
            .build();

        // Should not create base record if no base values
        assert_eq!(pack.records.len(), 1);
        assert_eq!(pack.records[0].v, Some(42.0));
    }
}