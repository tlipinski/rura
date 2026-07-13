use itertools::Itertools;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Rura {
    pub steps: Vec<String>,
}

impl Rura {
    pub fn empty() -> Self {
        Self { steps: vec![] }
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn to_string(&self) -> String {
        self.steps.join("|")
    }

    pub fn trimmed(&self) -> Vec<String> {
        self.steps.iter().map(|s| s.trim().into()).collect_vec()
    }
}

impl From<Vec<String>> for Rura {
    fn from(to_run: Vec<String>) -> Self {
        Self { steps: to_run }
    }
}

impl From<&str> for Rura {
    fn from(to_run: &str) -> Self {
        Self {
            steps: vec![to_run.into()],
        }
    }
}
