// System audio mute on macOS: the two platform calls behind audio/mute.rs.
//
// Reference VoiceInk : MediaController.swift (mutes the default output
// device). Here the system's own `osascript` volume commands do the same:
// "set volume output muted" and "output muted of (get volume settings)" need
// no extra library and no Automation permission, since they address the
// system volume and not another app.
//
// A device with no mute control (some HDMI and USB outputs) reports "missing
// value"; that is an error here, and mute.rs then skips muting instead of
// guessing, the same as on Windows.

use std::process::Command;

use anyhow::{anyhow, Context, Result};

const OSASCRIPT: &str = "/usr/bin/osascript";

fn run(script: &str) -> Result<String> {
    let out = Command::new(OSASCRIPT)
        .args(["-e", script])
        .output()
        .context("osascript could not start")?;
    if !out.status.success() {
        return Err(anyhow!(
            "osascript failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Reads what `output muted of (get volume settings)` printed.
fn parse_muted(output: &str) -> Result<bool> {
    match output.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(anyhow!("this output device cannot be muted ({other})")),
    }
}

pub fn read_mute() -> Result<bool> {
    parse_muted(&run("output muted of (get volume settings)")?)
}

pub fn write_mute(muted: bool) -> Result<()> {
    run(&format!("set volume output muted {muted}")).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_muted_reads_true_and_false() {
        assert!(parse_muted("true\n").unwrap());
        assert!(!parse_muted("false").unwrap());
    }

    #[test]
    fn parse_muted_rejects_devices_without_mute() {
        assert!(parse_muted("missing value").is_err());
        assert!(parse_muted("").is_err());
    }
}
