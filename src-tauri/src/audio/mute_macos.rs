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
//
// Device limitation: osascript only reaches the current default output, so
// there is one endpoint ID (`DEFAULT_OUTPUT_ID`) and the restore un-mutes
// whatever the default output is at that moment. VoiceInk restores the exact
// device it muted (Core Audio device IDs); that would need a Core Audio crate
// and is not done here. If the user switches output during a dictation, the
// output Parlato muted stays muted on the Mac (the Windows build restores it
// by ID). The rest of the state machine in mute.rs (sessions, ownership,
// restore even if the setting was turned off) is shared with Windows.

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

/// The only endpoint ID on the Mac: "the current default output".
pub const DEFAULT_OUTPUT_ID: &str = "default-output";

/// ID and mute state of the default output (same shape as the Windows call).
pub fn default_output_state() -> Result<(String, bool)> {
    Ok((DEFAULT_OUTPUT_ID.to_string(), read_mute()?))
}

/// Mutes or un-mutes the default output. `_id` is always `DEFAULT_OUTPUT_ID`.
pub fn set_output_muted(_id: &str, muted: bool) -> Result<()> {
    write_mute(muted)
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
