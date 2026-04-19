use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const INSTALL_SCRIPT_URL: &str = "https://raw.githubusercontent.com/oneqit/qmux/main/install.sh";
const LATEST_RELEASE_API_URL: &str = "https://api.github.com/repos/oneqit/qmux/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct UpdateOptions {
    version: Option<String>,
    bin_dir: Option<PathBuf>,
    no_verify: bool,
    help: bool,
}

pub fn run_update(args: &[String]) -> Result<()> {
    let opts = parse_update_args(args)?;
    if opts.help {
        print!("{}", update_help());
        return Ok(());
    }
    if opts.version.is_none() {
        match latest_release_tag() {
            Ok(tag) => {
                let current_tag = format!("v{}", env!("CARGO_PKG_VERSION"));
                if tag == current_tag {
                    println!("qmux {} is already up to date", env!("CARGO_PKG_VERSION"));
                    return Ok(());
                }
            }
            Err(err) => {
                eprintln!(
                    "warning: failed to check latest qmux version: {err}; continuing with update"
                );
            }
        }
    }

    let bin_dir = match opts.bin_dir {
        Some(p) => p,
        None => default_bin_dir()?,
    };
    let mut sh = Command::new("sh");
    sh.arg("-s").arg("--").arg("--bin-dir").arg(&bin_dir);
    if let Some(version) = opts.version.as_deref() {
        sh.arg("--version").arg(version);
    }
    if opts.no_verify {
        sh.arg("--no-verify");
    }

    let mut curl = Command::new("curl")
        .arg("-fsSL")
        .arg(INSTALL_SCRIPT_URL)
        .stdout(Stdio::piped())
        .spawn()
        .context("starting curl for installer script")?;
    let Some(curl_stdout) = curl.stdout.take() else {
        anyhow::bail!("failed to capture curl stdout for installer script");
    };

    let sh_status = sh
        .stdin(Stdio::from(curl_stdout))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("running qmux installer script")?;
    let curl_status = curl.wait().context("waiting for curl process")?;

    if !curl_status.success() {
        anyhow::bail!("qmux update failed while downloading installer script");
    }
    if !sh_status.success() {
        anyhow::bail!("qmux update failed (installer exited with status {sh_status})");
    }
    Ok(())
}

fn parse_update_args(args: &[String]) -> Result<UpdateOptions> {
    let mut opts = UpdateOptions::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                opts.help = true;
                i += 1;
            }
            "--version" => {
                let Some(raw) = args.get(i + 1) else {
                    anyhow::bail!("--version requires a value (example: --version v0.1.0)");
                };
                opts.version = Some(normalize_version(raw));
                i += 2;
            }
            "--bin-dir" => {
                let Some(dir) = args.get(i + 1) else {
                    anyhow::bail!("--bin-dir requires a value");
                };
                opts.bin_dir = Some(PathBuf::from(dir));
                i += 2;
            }
            "--no-verify" => {
                opts.no_verify = true;
                i += 1;
            }
            unknown => {
                anyhow::bail!("unknown option for `qmux update`: {unknown}");
            }
        }
    }
    Ok(opts)
}

fn normalize_version(version: &str) -> String {
    if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    }
}

fn default_bin_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("resolving current qmux executable path")?;
    let Some(parent) = exe.parent() else {
        anyhow::bail!("cannot determine binary directory for {}", exe.display());
    };
    Ok(parent.to_path_buf())
}

fn latest_release_tag() -> Result<String> {
    let output = Command::new("curl")
        .arg("-fsSL")
        .arg(LATEST_RELEASE_API_URL)
        .output()
        .context("fetching latest release metadata from GitHub")?;
    if !output.status.success() {
        anyhow::bail!("GitHub API request failed with status {}", output.status);
    }
    let body = String::from_utf8(output.stdout).context("decoding GitHub API response as UTF-8")?;
    extract_tag_name(&body)
}

fn extract_tag_name(json: &str) -> Result<String> {
    let key = "\"tag_name\"";
    let Some(key_idx) = json.find(key) else {
        anyhow::bail!("`tag_name` not found in GitHub API response");
    };
    let after_key = &json[key_idx + key.len()..];
    let Some(colon_idx) = after_key.find(':') else {
        anyhow::bail!("malformed GitHub API response around `tag_name`");
    };
    let value = after_key[colon_idx + 1..].trim_start();
    if !value.starts_with('"') {
        anyhow::bail!("`tag_name` value is not a string");
    }
    let value = &value[1..];
    let Some(end_idx) = value.find('"') else {
        anyhow::bail!("unterminated `tag_name` string in GitHub API response");
    };
    let tag = value[..end_idx].trim();
    if tag.is_empty() {
        anyhow::bail!("empty `tag_name` in GitHub API response");
    }
    Ok(normalize_version(tag))
}

pub fn update_help() -> &'static str {
    "Update qmux in-place using the official installer.\n\
     \n\
     Usage:\n\
       qmux update [options]\n\
     \n\
     Options:\n\
       --version <vX.Y.Z>  Install a specific release (default: latest)\n\
       --bin-dir <path>    Install directory (default: current qmux binary dir)\n\
       --no-verify         Skip checksum verification\n\
       -h, --help          Show this help\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parse_defaults() {
        let opts = parse_update_args(&[]).unwrap();
        assert_eq!(opts.version, None);
        assert_eq!(opts.bin_dir, None);
        assert!(!opts.no_verify);
        assert!(!opts.help);
    }

    #[test]
    fn parse_with_version_and_normalize() {
        let opts = parse_update_args(&s(&["--version", "1.2.3"])).unwrap();
        assert_eq!(opts.version.as_deref(), Some("v1.2.3"));
    }

    #[test]
    fn parse_with_bin_dir_and_no_verify() {
        let opts = parse_update_args(&s(&["--bin-dir", "/tmp/bin", "--no-verify"])).unwrap();
        assert_eq!(opts.bin_dir, Some(PathBuf::from("/tmp/bin")));
        assert!(opts.no_verify);
    }

    #[test]
    fn parse_help_flag() {
        let opts = parse_update_args(&s(&["--help"])).unwrap();
        assert!(opts.help);
    }

    #[test]
    fn parse_errors_on_missing_values_and_unknown_options() {
        assert!(parse_update_args(&s(&["--version"])).is_err());
        assert!(parse_update_args(&s(&["--bin-dir"])).is_err());
        assert!(parse_update_args(&s(&["--wat"])).is_err());
    }

    #[test]
    fn extract_tag_name_parses_and_normalizes() {
        let raw = r#"{"id":1,"tag_name":"0.1.1"}"#;
        assert_eq!(extract_tag_name(raw).unwrap(), "v0.1.1");
    }

    #[test]
    fn extract_tag_name_preserves_v_prefix() {
        let raw = r#"{"tag_name":"v0.1.1"}"#;
        assert_eq!(extract_tag_name(raw).unwrap(), "v0.1.1");
    }

    #[test]
    fn extract_tag_name_errors_for_missing_or_invalid_value() {
        assert!(extract_tag_name(r#"{"name":"v0.1.1"}"#).is_err());
        assert!(extract_tag_name(r#"{"tag_name":1}"#).is_err());
    }
}
