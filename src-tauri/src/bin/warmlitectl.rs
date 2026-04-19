use std::env;
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, anyhow, bail};
use shared::{MonitorControl, MonitorSnapshot};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("warmlitectl: {error:#}");
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(|| anyhow!("missing command"))?;
    let rest: Vec<String> = args.collect();

    match command.as_str() {
        "list" => cmd_list(),
        "brightness" => cmd_brightness(rest),
        "scene" => cmd_scene(rest),
        _ => bail!("unknown command: {command}"),
    }
}

fn cmd_list() -> Result<()> {
    let monitors = warmlite::list_monitors_blocking()?;
    if monitors.is_empty() {
        println!("No monitors found.");
        return Ok(());
    }

    for monitor in monitors {
        let brightness_text = find_control(&monitor, "10")
            .and_then(brightness_percent)
            .map(|value| format!("{value}%"))
            .unwrap_or_else(|| String::from("n/a"));
        println!(
            "{}\t{}\tbrightness={}",
            monitor.id,
            monitor.label(),
            brightness_text
        );
    }
    Ok(())
}

fn cmd_brightness(args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("missing brightness subcommand");
    }

    let subcommand = args[0].as_str();
    let mut tail = args[1..].to_vec();
    let (monitor_query, notify) = parse_common_flags(&mut tail)?;

    match subcommand {
        "get" => {
            if !tail.is_empty() {
                bail!("brightness get does not accept positional arguments");
            }
            let monitor = select_monitor(monitor_query.as_deref())?;
            let percent = monitor_brightness_percent(&monitor)?;
            println!("{percent}");
        }
        "set" => {
            if tail.len() != 1 {
                bail!("usage: warmlitectl brightness set <0-100> [--monitor QUERY] [--notify]");
            }
            let target = parse_u16(&tail[0], "brightness value")?.min(100);
            let monitor = select_monitor(monitor_query.as_deref())?;
            let updated = warmlite::set_monitor_feature_blocking(&monitor.id, "10", target)?;
            let percent = monitor_brightness_percent(&updated).unwrap_or(target);
            println!("{percent}");
            if notify {
                maybe_notify_brightness(percent, updated.label());
            }
        }
        "delta" => {
            if tail.len() != 1 {
                bail!("usage: warmlitectl brightness delta <+N|-N> [--monitor QUERY] [--notify]");
            }
            let delta = parse_i16(&tail[0], "brightness delta")?;
            let monitor = select_monitor(monitor_query.as_deref())?;
            let current = monitor_brightness_percent(&monitor)?;
            let target = ((current as i32) + (delta as i32)).clamp(0, 100) as u16;
            let updated = warmlite::set_monitor_feature_blocking(&monitor.id, "10", target)?;
            let percent = monitor_brightness_percent(&updated).unwrap_or(target);
            println!("{percent}");
            if notify {
                maybe_notify_brightness(percent, updated.label());
            }
        }
        _ => bail!("unknown brightness subcommand: {subcommand}"),
    }

    Ok(())
}

fn cmd_scene(args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!(
            "usage: warmlitectl scene <paper|sunset|ember|incandescent|candle|nocturne> [--monitor QUERY]"
        );
    }
    let scene_id = args[0].clone();
    let mut tail = args[1..].to_vec();
    let (monitor_query, notify) = parse_common_flags(&mut tail)?;
    if !tail.is_empty() {
        bail!("scene only accepts one positional argument");
    }

    let monitor = select_monitor(monitor_query.as_deref())?;
    let updated = warmlite::apply_color_scene_blocking(&monitor.id, &scene_id)?;
    println!("{} {}", updated.id, scene_id);
    if notify {
        let title = format!("Scene: {scene_id}");
        maybe_notify_text(title.as_str(), updated.label().as_str());
    }
    Ok(())
}

fn parse_common_flags(args: &mut Vec<String>) -> Result<(Option<String>, bool)> {
    let mut monitor_query = None;
    let mut notify = false;
    let mut positionals = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--monitor" | "-m" => {
                index += 1;
                let query = args
                    .get(index)
                    .ok_or_else(|| anyhow!("missing value for --monitor"))?;
                monitor_query = Some(query.clone());
            }
            "--notify" | "-n" => {
                notify = true;
            }
            value => {
                positionals.push(value.to_string());
            }
        }
        index += 1;
    }
    *args = positionals;
    Ok((monitor_query, notify))
}

fn select_monitor(query: Option<&str>) -> Result<MonitorSnapshot> {
    let monitors = warmlite::list_monitors_blocking()?;
    if monitors.is_empty() {
        bail!("no monitors detected");
    }

    if let Some(query) = query {
        let query_lower = query.to_ascii_lowercase();
        let matches: Vec<MonitorSnapshot> = monitors
            .iter()
            .filter(|monitor| {
                monitor.id.eq_ignore_ascii_case(query)
                    || monitor.id.to_ascii_lowercase().contains(&query_lower)
                    || monitor
                        .model_name
                        .as_deref()
                        .map(|model| model.to_ascii_lowercase().contains(&query_lower))
                        .unwrap_or(false)
                    || monitor.label().to_ascii_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();

        return match matches.len() {
            0 => bail!("no monitor matched query: {query}"),
            1 => Ok(matches[0].clone()),
            _ => {
                let ids: Vec<String> = matches.iter().map(|monitor| monitor.id.clone()).collect();
                bail!(
                    "monitor query is ambiguous: {query} (matches: {})",
                    ids.join(", ")
                )
            }
        };
    }

    if let Some(ddc_monitor) = monitors
        .iter()
        .find(|monitor| monitor.id.starts_with("ddc:") && monitor.supports_controls())
    {
        return Ok(ddc_monitor.clone());
    }
    if let Some(any_ddc_monitor) = monitors
        .iter()
        .find(|monitor| monitor.id.starts_with("ddc:"))
    {
        return Ok(any_ddc_monitor.clone());
    }
    Ok(monitors[0].clone())
}

fn find_control<'a>(monitor: &'a MonitorSnapshot, code: &str) -> Option<&'a MonitorControl> {
    monitor
        .controls
        .iter()
        .find(|control| control.code.eq_ignore_ascii_case(code))
}

fn monitor_brightness_percent(monitor: &MonitorSnapshot) -> Result<u16> {
    let control = find_control(monitor, "10")
        .with_context(|| format!("monitor {} does not expose brightness control", monitor.id))?;
    brightness_percent(control).ok_or_else(|| {
        anyhow!(
            "monitor {} brightness is unavailable ({})",
            monitor.id,
            control
                .error
                .clone()
                .unwrap_or_else(|| String::from("no current value"))
        )
    })
}

fn brightness_percent(control: &MonitorControl) -> Option<u16> {
    let current = control.current_value?;
    let max = control.max_value.unwrap_or(100).max(1);
    if max <= 100 {
        return Some(current.min(100));
    }
    let normalized = ((current as u32) * 100 + ((max as u32) / 2)) / (max as u32);
    Some(normalized.min(100) as u16)
}

fn parse_u16(input: &str, what: &str) -> Result<u16> {
    input
        .parse::<u16>()
        .with_context(|| format!("invalid {what}: {input}"))
}

fn parse_i16(input: &str, what: &str) -> Result<i16> {
    input
        .parse::<i16>()
        .with_context(|| format!("invalid {what}: {input}"))
}

fn maybe_notify_brightness(percent: u16, monitor_label: String) {
    let body = format!("{percent}% · {monitor_label}");
    maybe_notify_text_with_value("Brightness", body.as_str(), percent);
}

fn maybe_notify_text(title: &str, body: &str) {
    maybe_notify_text_with_value(title, body, 0);
}

fn maybe_notify_text_with_value(title: &str, body: &str, value: u16) {
    let mut command = Command::new("notify-send");
    command
        .arg("-a")
        .arg("warmlite")
        .arg("-h")
        .arg("string:x-canonical-private-synchronous:warmlite-brightness");
    if value > 0 {
        command.arg("-h").arg(format!("int:value:{value}"));
    }
    let status = command.arg(title).arg(body).status();
    if let Err(error) = status {
        eprintln!("warmlitectl: notify-send failed: {error}");
    }
}

fn print_usage() {
    eprintln!(
        "Usage:
  warmlitectl list
  warmlitectl brightness get [--monitor QUERY]
  warmlitectl brightness set <0-100> [--monitor QUERY] [--notify]
  warmlitectl brightness delta <+N|-N> [--monitor QUERY] [--notify]
  warmlitectl scene <paper|sunset|ember|incandescent|candle|nocturne> [--monitor QUERY] [--notify]
"
    );
}
