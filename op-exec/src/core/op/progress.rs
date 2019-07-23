#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Unit {
    Step,
    Scalar,
    Percent,
    Bytes,
    Seconds,
}

impl Unit {
    pub fn symbol(&self) -> &str {
        match *self {
            Unit::Step => "",
            Unit::Scalar => "",
            Unit::Percent => "%",
            Unit::Bytes => "B",
            Unit::Seconds => "sec",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    value: f64,
    min: f64,
    max: f64,
    unit: Unit,
    steps: Vec<Progress>,
    counter: u32,
    file_name: Option<String>,
}

impl Progress {
    pub fn new(min: f64, max: f64, unit: Unit) -> Progress {
        Progress {
            value: min,
            min,
            max,
            unit,
            steps: Vec::new(),
            counter: 0,
            file_name: None,
        }
    }

    pub fn with_file_name(min: f64, max: f64, unit: Unit, file_name: String) -> Progress {
        Progress {
            file_name: Some(file_name),
            ..Progress::new(min, max, unit)
        }
    }

    pub fn from_steps(steps: Vec<Progress>) -> Progress {
        let mut units: Vec<_> = steps.iter().map(|p| p.unit).collect();
        units.sort();
        units.dedup();
        if units.len() == 1 {
            Progress {
                value: steps[0].min,
                min: steps[0].min,
                max: steps.iter().fold(0., |max, s| max + s.max - s.min),
                unit: units[0],
                steps,
                counter: 0,
                file_name: None,
            }
        } else {
            Progress {
                value: 1.,
                min: 1.,
                max: steps.len() as f64,
                unit: Unit::Step,
                steps,
                counter: 0,
                file_name: None,
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
        if self.value != value {
            self.value = value;
            self.counter += 1;
            true
        } else {
            false
        }
    }

    pub fn set_value_done(&mut self) -> bool {
        for s in &mut self.steps {
            s.set_value_done();
        }
        if self.value != self.max {
            self.value = self.max;
            self.counter += 1;
            true
        } else {
            false
        }
    }

    pub fn set_step_value(&mut self, step: usize, value: f64) -> bool {
        let u = self.steps[step].set_value(value);
        if u {
            if self.unit == Unit::Step {
                self.value = self
                    .steps
                    .iter()
                    .fold(1., |v, s| v + s.is_done() as u32 as f64);
            } else {
                self.value = self.steps.iter().fold(0., |v, s| v + s.value - s.min);
            }
            self.counter += 1;
        }
        u
    }

    pub fn set_step(&mut self, step: usize, progress: Progress) {
        self.steps[step] = progress;
        if self.unit == Unit::Step {
            self.value = self
                .steps
                .iter()
                .fold(1., |v, s| v + s.is_done() as u32 as f64);
        } else {
            self.value = self.steps.iter().fold(0., |v, s| v + s.value - s.min);
        }
        self.counter += 1;
    }

    pub fn set_step_value_done(&mut self, step: usize) -> bool {
        let value = self.steps[step].max;
        self.set_step_value(step, value)
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

    pub fn steps(&self) -> &[Progress] {
        &self.steps
    }

    pub fn file_name(&self) -> Option<&String> {
        self.file_name.as_ref()
    }

    pub(super) fn counter(&self) -> u32 {
        self.counter
    }
}

impl Default for Progress {
    fn default() -> Self {
        Progress {
            value: 0.,
            min: 0.,
            max: 999999999999.,
            unit: Unit::Scalar,
            steps: Vec::new(),
            counter: 0,
            file_name: None,
        }
    }
}

impl std::fmt::Display for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if f.alternate() {
            for (i, s) in self.steps.iter().enumerate() {
                write!(f, "({}) {}\n", i + 1, s)?;
            }
            write!(f, "{} / {} {}\n", self.value, self.max, self.unit.symbol())
        } else {
            write!(f, "{} / {} {}", self.value, self.max, self.unit.symbol())
        }
    }
}
