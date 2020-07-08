use kg_utils::collections::LinkedHashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Unit {
    Scalar,
    Percent,
    Bytes,
    Seconds,
}

impl Unit {
    pub fn symbol(&self) -> &str {
        match *self {
            Unit::Scalar => "",
            Unit::Percent => "%",
            Unit::Bytes => "B",
            Unit::Seconds => "s",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    counter: u32,
    unit: Unit,
    min: f64,
    max: f64,
    value: f64,
    speed: Option<f64>,
    label: Option<String>,
    parts: LinkedHashMap<String, Progress>,
}

impl Progress {
    pub fn new(min: f64, max: f64, unit: Unit) -> Progress {
        Progress {
            counter: 0,
            unit,
            value: min,
            min,
            max,
            speed: None,
            label: None,
            parts: LinkedHashMap::new(),
        }
    }
    pub fn new_partial(label: &str, min: f64, max: f64, unit: Unit) -> Progress {
        Progress {
            label: Some(label.to_string()),
            ..Progress::new(min, max, unit)
        }
    }

    /// Create progress from parts. Panics if parts without `label` provided.
    pub fn from_parts(parts: Vec<Progress>) -> Progress {
        let mut units: Vec<_> = parts.iter().map(|p| p.unit).collect();
        units.sort();
        units.dedup();
        if units.len() == 1 {
            Progress {
                value: parts[0].min,
                min: parts[0].min,
                max: parts.iter().fold(0., |max, s| max + s.max - s.min),
                unit: units[0],
                counter: 0,
                label: None,
                speed: None,
                parts: parts
                    .into_iter()
                    .map(|p| (p.label.as_ref().cloned().take().unwrap(), p))
                    .collect(),
            }
        } else {
            Progress {
                value: 1.,
                min: 1.,
                max: parts.len() as f64,
                unit: Unit::Scalar,
                counter: 0,
                label: None,
                speed: None,
                parts: parts
                    .into_iter()
                    .map(|mut p| (p.label.take().unwrap(), p))
                    .collect(),
            }
        }
    }

    pub fn is_done(&self) -> bool {
        self.value >= self.max
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn set_value(&mut self, mut value: f64) -> bool {
        if value > self.max {
            value = self.max;
        } else if value < self.min {
            value = self.min;
        }
        if (self.value - value).abs() > std::f64::EPSILON {
            self.value = value;
            self.counter += 1;
            true
        } else {
            false
        }
    }

    pub fn set_value_done(&mut self) -> bool {
        self.set_value(self.max)
    }

    pub fn min(&self) -> f64 {
        self.min
    }

    pub fn max(&self) -> f64 {
        self.max
    }

    pub fn unit(&self) -> Unit {
        self.unit
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn set_label<S: Into<String>>(&mut self, label: S) {
        self.label = Some(label.into())
    }

    pub fn speed(&self) -> Option<f64> {
        self.speed
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.speed = Some(speed)
    }

    pub fn update(&mut self, u: ProgressUpdate) {
        match u {
            ProgressUpdate::Main {
                value,
                speed,
                label,
            } => {
                if value.is_finite() {
                    self.set_value(value);
                } else {
                    self.set_value_done();
                }
                if speed.is_some() {
                    self.speed = speed;
                }
                if label.is_some() {
                    self.label = label;
                }
            }
            ProgressUpdate::Partial {
                value,
                speed,
                label,
            } => {
                if !self.parts.contains_key(&label) {
                    let mut part = Progress::default();
                    part.unit = self.unit;
                    self.parts.insert(label.clone(), part);
                }

                if let Some(part) = self.parts.get_mut(&label) {
                    if value.is_finite() {
                        part.set_value(value);
                    } else {
                        part.set_value_done();
                    }
                    if speed.is_some() {
                        part.speed = speed;
                    }
                    part.label = Some(label.clone());
                }
                self.label = Some(label);
                self.update_from_parts()
            }
        }
    }

    pub fn counter(&self) -> u32 {
        self.counter
    }

    fn update_from_parts(&mut self) {
        // let total = self.parts.values().iter().fold(0., |max, s| max + s.max - s.min);
        let value = self.parts.iter().fold(0., |total, s| total + s.1.value);
        if value.is_finite() {
            self.set_value(value);
        } else {
            self.set_value_done();
        }
    }
}

impl Default for Progress {
    fn default() -> Self {
        Progress {
            counter: 0,
            unit: Unit::Percent,
            value: 0.,
            min: 0.,
            max: 100.,
            speed: None,
            label: None,
            parts: LinkedHashMap::new(),
        }
    }
}

impl std::fmt::Display for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let symbol = self.unit.symbol();

        if let Some(ref label) = self.label {
            write!(
                f,
                "{}{} / {}{} {}",
                self.value,
                symbol,
                (self.max - self.min),
                symbol,
                label
            )
        } else {
            write!(
                f,
                "{}{} / {}{}",
                self.value,
                symbol,
                (self.max - self.min),
                symbol
            )
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProgressUpdate {
    Main {
        value: f64,
        speed: Option<f64>,
        label: Option<String>,
    },
    Partial {
        value: f64,
        speed: Option<f64>,
        label: String,
    },
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct ProgressUpdate {
//     is_partial: bool,
//     value: f64,
//     speed: Option<f64>,
//     label: Option<String>,
// }

impl ProgressUpdate {
    pub fn new(value: f64) -> ProgressUpdate {
        ProgressUpdate::Main {
            value,
            speed: None,
            label: None,
        }
    }

    pub fn new_partial(value: f64, label: String) -> Self {
        ProgressUpdate::Partial {
            value,
            label,
            speed: None,
        }
    }

    pub fn done() -> ProgressUpdate {
        ProgressUpdate::Main {
            value: std::f64::NAN,
            speed: None,
            label: None,
        }
    }

    pub fn partial_done(label: String) -> ProgressUpdate {
        ProgressUpdate::Partial {
            value: std::f64::NAN,
            speed: None,
            label,
        }
    }

    pub fn value(&self) -> f64 {
        match self {
            ProgressUpdate::Main { value, .. } => *value,
            ProgressUpdate::Partial { value, .. } => *value,
        }
    }

    pub fn speed(&self) -> Option<f64> {
        match self {
            ProgressUpdate::Main { speed, .. } => speed.as_ref().copied(),
            ProgressUpdate::Partial { speed, .. } => speed.as_ref().copied(),
        }
    }

    // pub fn with_speed(mut self, speed: f64) -> ProgressUpdate {
    //     self.speed = Some(speed);
    //     self
    // }
    //
    // pub fn with_label<S: Into<String>>(mut self, label: S) -> ProgressUpdate {
    //     self.label = Some(label.into());
    //     self
    // }

    pub fn set_label(&mut self, new_label: String) {
        match self {
            ProgressUpdate::Main { ref mut label, .. } => *label = Some(new_label),
            ProgressUpdate::Partial { ref mut label, .. } => *label = new_label,
        }
    }
}
