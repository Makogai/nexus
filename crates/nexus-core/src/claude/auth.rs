//! Authentication status via the supported `claude auth status --json` (run with the profile's environment).
//! NEXUS never reads credential files. The returned identity is kept in memory only (never stored or logged).

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::claude::launch::profile_env;
use crate::claude::profile::ClaudeProfile;
use crate::error::{CoreError, Result};
use crate::{env, proc};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    #[serde(default)]
    pub logged_in: bool,
    #[serde(default)]
    pub auth_method: Option<String>,
    #[serde(default)]
    pub api_provider: Option<String>,
    #[serde(default)]
    pub subscription_type: Option<String>,
    #[serde(default)]
    pub org_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

pub fn parse(stdout: &str) -> Option<AuthStatus> {
    let start = stdout.find('{')?;
    serde_json::from_str::<AuthStatus>(&stdout[start..]).ok()
}

pub fn status(executable: &Path, profile: &ClaudeProfile) -> Result<AuthStatus> {
    let mut cmd = proc::command(executable);
    cmd.args(["auth", "status", "--json"]);
    let (child_env, _) = profile_env(profile, &env::current(), &[]);
    env::apply(&mut cmd, &child_env);
    let out = proc::run(cmd, Duration::from_secs(25))?;
    // A logged-out profile may exit non-zero but still print JSON.
    parse(&out.stdout_str()).ok_or_else(|| CoreError::External {
        program: "claude auth status".into(),
        message: out
            .stderr_str()
            .lines()
            .next()
            .map(|l| l.chars().take(200).collect())
            .unwrap_or_else(|| "unexpected output".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_fields_and_ignores_others() {
        let s = parse(
            r#"{"loggedIn":true,"authMethod":"claude.ai","subscriptionType":"max","orgName":"Org",
                "email":"a@b.c","configDirectory":"/x","somethingNew":42}"#,
        )
        .unwrap();
        assert!(s.logged_in);
        assert_eq!(s.subscription_type.as_deref(), Some("max"));
        assert!(parse("not json").is_none());
        assert!(!parse(r#"{"loggedIn":false}"#).unwrap().logged_in);
    }
}
