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
        }
    }

    /*pub fn from_steps(steps: Vec<Progress>) -> Progress {
        let mut units: Vec<_> = steps.iter().map(|p| p.unit).collect();
        units.sort();
        units.dedup();
        if units.len() == 1 {
            Progress {
                value: steps[0].min,
                min: steps[0].min,
                max: steps.iter().fold(0., |max, s| max + s.max - s.min),
                unit: units[0],
                counter: 0,
                file_name: None,
            }
        } else {
            Progress {
                value: 1.,
                min: 1.,
                max: steps.len() as f64,
                unit: Unit::Step,
                counter: 0,
                file_name: None,
            }
        }
    }*/

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
        self.label.as_ref().map(String::as_str)
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
        if u.value.is_finite() {
            self.set_value(u.value);
        } else {
            self.set_value_done();
        }
        if u.speed.is_some() {
            self.speed = u.speed;
        }
        if u.label.is_some() {
            self.label = u.label;
        }
    }

    pub(super) fn counter(&self) -> u32 {
        self.counter
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
        }
    }
}

impl std::fmt::Display for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let symbol = self.unit.symbol();
        write!(f, "{}{} / {}{}", self.value, symbol, (self.max - self.min), symbol)
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressUpdate {
    value: f64,
    speed: Option<f64>,
    label: Option<String>,
}

impl ProgressUpdate {
    pub fn new(value: f64) -> ProgressUpdate {
        ProgressUpdate {
            value,
            speed: None,
            label: None,
        }
    }

    pub fn done() -> ProgressUpdate {
        ProgressUpdate {
            value: std::f64::NAN,
            speed: None,
            label: None,
        }
    }

    pub fn with_speed(mut self, speed: f64) -> ProgressUpdate {
        self.speed = Some(speed);
        self
    }

    pub fn with_label<S: Into<String>>(mut self, label: S) -> ProgressUpdate {
        self.label = Some(label.into());
        self
    }
}