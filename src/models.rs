use anyhow::Error;
use regex::Regex;

pub struct Regexes {
    pub tag: Regex,
    pub main_branches: Regex,
    pub rc_branches: Regex,
    pub develop_branches: Regex,
    pub escape_branch: Regex,
}

impl Regexes {
    pub fn new(app_name: &Option<String>) -> Result<Self, Error> {
        let tag = if app_name.is_none() {
            Regex::new(r"(?<version>.+)$")?
        } else {
            Regex::new(&format!(r"^{}-(?<version>.+)$", app_name.as_ref().unwrap()))?
        };
        let main_branches = Regex::new(r"^main|master$").unwrap();
        let rc_branches = if app_name.is_none() {
            Regex::new(r"^(hotfix\/|release\/)(?<version>.+)")?
        } else {
            Regex::new(&format!(
                r"^(hotfix\/|release\/){}-(?<version>.+)",
                app_name.as_ref().unwrap()
            ))?
        };
        let develop_branches = Regex::new(r"^develop|dev$").unwrap();
        let escape_branch = Regex::new(r"[^a-zA-Z0-9-]").unwrap();

        Ok(Self {
            tag,
            main_branches,
            rc_branches,
            develop_branches,
            escape_branch,
        })
    }
}
