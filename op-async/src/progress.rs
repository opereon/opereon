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
            Unit::Seconds => "sec",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    counter: u32,
    value: f64,
    min: f64,
    max: f64,
    unit: Unit,
    label: Option<String>,
}

impl Progress {
    pub fn new(min: f64, max: f64, unit: Unit) -> Progress {
        Progress {
            counter: 0,
            value: min,
            min,
            max,
            unit,
            label: None,
        }
    }

    pub fn with_file_name<S: Into<String>>(min: f64, max: f64, unit: Unit, file_name: S) -> Progress {
        Progress {
            label: Some(file_name.into()),
            ..Progress::new(min, max, unit)
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

    pub(super) fn counter(&self) -> u32 {
        self.counter
    }
}

impl Default for Progress {
    fn default() -> Self {
        Progress {
            counter: 0,
            value: 0.,
            min: 0.,
            max: 999999999999.,
            unit: Unit::Scalar,
            label: None,
        }
    }
}

impl std::fmt::Display for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} / {} {}", self.value, self.max, self.unit.symbol())
    }
}
