use leptos::ev::Event;
use leptos::prelude::*;
use shared::{ControlOption, MonitorControl, MonitorControlType, MonitorSnapshot};
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlInputElement, window};

use crate::interop;

#[derive(Clone)]
struct RangeChangeContext {
    monitor_id: String,
    control_code: String,
    delay: u64,
    slider_value: RwSignal<u16>,
    pending_value: RwSignal<Option<u16>>,
    in_flight: RwSignal<bool>,
    monitors: RwSignal<Vec<MonitorSnapshot>>,
    local_error: RwSignal<String>,
    control_label: String,
}

#[derive(Clone, Copy)]
struct ColorSceneOption {
    id: &'static str,
    label: &'static str,
}

const COLOR_SCENES: &[ColorSceneOption] = &[
    ColorSceneOption {
        id: "paper",
        label: "Paper",
    },
    ColorSceneOption {
        id: "sunset",
        label: "Sunset",
    },
    ColorSceneOption {
        id: "ember",
        label: "Ember",
    },
    ColorSceneOption {
        id: "incandescent",
        label: "Incandescent",
    },
    ColorSceneOption {
        id: "candle",
        label: "Candle",
    },
    ColorSceneOption {
        id: "nocturne",
        label: "Nocturne",
    },
];

#[component]
pub fn App() -> impl IntoView {
    let monitors = RwSignal::new(Vec::<MonitorSnapshot>::new());
    let selected_monitor_id = RwSignal::new(None::<String>);
    let status = RwSignal::new(String::from("Scanning displays..."));
    let is_loading = RwSignal::new(false);
    let glide_delay_ms = RwSignal::new(18_u16);

    let refresh = move || {
        is_loading.set(true);
        status.set(String::from("Refreshing monitor state..."));

        spawn_local(async move {
            match interop::list_monitors().await {
                Ok(next) => {
                    let count = next.len();
                    let current_selection = selected_monitor_id.get();
                    let next_selection = current_selection
                        .filter(|selected| next.iter().any(|monitor| &monitor.id == selected))
                        .or_else(|| next.first().map(|monitor| monitor.id.clone()));

                    monitors.set(next);
                    selected_monitor_id.set(next_selection);
                    status.set(match count {
                        0 => String::from("No displays responded to DDC."),
                        1 => String::from("1 display ready."),
                        _ => format!("{count} displays ready."),
                    });
                }
                Err(error) => status.set(error),
            }

            is_loading.set(false);
        });
    };

    let selected_monitor = move || {
        let selection = selected_monitor_id.get();
        monitors
            .get()
            .into_iter()
            .find(|monitor| Some(monitor.id.clone()) == selection)
    };

    Effect::new(move |_| {
        refresh();
    });

    view! {
        <main class="shell scene-shell">
            <header class="scene-topbar">
                <div class="scene-brand">
                    <div class="brand-copy">
                        <p class="eyebrow">"WarmLight"</p>
                        <strong>"Display Studio"</strong>
                    </div>
                </div>

                <div class="topbar-tools">
                    <div class="glide-inline">
                        <div class="glide-copy">
                            <span class="panel-label">"Glide"</span>
                            <strong>{move || glide_label(glide_delay_ms.get())}</strong>
                        </div>
                        <input
                            class="slider slim"
                            type="range"
                            min="0"
                            max="50"
                            step="2"
                            prop:value=move || glide_delay_ms.get().to_string()
                            on:input=move |event: Event| {
                                let input = event_target::<HtmlInputElement>(&event);
                                if let Ok(parsed) = input.value().parse::<u16>() {
                                    glide_delay_ms.set(parsed);
                                }
                            }
                        />
                    </div>

                    <button
                        class="button ghost"
                        on:click=move |_| refresh()
                        disabled=move || is_loading.get()
                    >
                        {move || if is_loading.get() { "Refreshing..." } else { "Refresh" }}
                    </button>
                </div>
            </header>

            <div class=move || format!("status-pill {}", if status.get().starts_with("Failed") { "error" } else { "" })>
                {move || status.get()}
            </div>

            <section class="display-switcher-bar">
                <div class="dock-label top">
                    <span>"Displays"</span>
                    <strong>{move || monitors.get().len()}</strong>
                </div>

                <div class="display-switcher">
                    <For
                        each=move || monitors.get()
                        key=|monitor| monitor.id.clone()
                        children=move |monitor| {
                            let monitor_id = monitor.id.clone();
                            let monitor_id_for_class = monitor_id.clone();
                            let monitor_id_for_click = monitor_id.clone();
                            let monitor_name = monitor.label();
                            let meta = monitor
                                .serial_number
                                .clone()
                                .or_else(|| monitor.manufacturer_id.clone())
                                .unwrap_or_else(|| monitor.backend.clone());

                            view! {
                                <button
                                    class=move || {
                                        if selected_monitor_id.get().as_deref() == Some(monitor_id_for_class.as_str()) {
                                            "display-switch active"
                                        } else {
                                            "display-switch"
                                        }
                                    }
                                    on:click=move |_| selected_monitor_id.set(Some(monitor_id_for_click.clone()))
                                >
                                    <span class="display-switch-name">{monitor_name}</span>
                                    <span class="display-switch-meta">{meta}</span>
                                </button>
                            }
                        }
                    />
                </div>
            </section>

            <section class="scene-stage">
                {move || {
                    if let Some(monitor) = selected_monitor() {
                        view! {
                            <MonitorStage
                                monitor
                                monitors
                                glide_delay_ms
                            />
                        }
                        .into_any()
                    } else {
                        view! {
                            <div class="empty-scene">
                                <p class="screen-kicker">"No display selected"</p>
                                <h2>"Waiting for a monitor"</h2>
                                <p>"Refresh after connecting a DDC-capable display."</p>
                            </div>
                        }
                        .into_any()
                    }
                }}
            </section>

        </main>
    }
}

#[component]
fn MonitorStage(
    monitor: MonitorSnapshot,
    monitors: RwSignal<Vec<MonitorSnapshot>>,
    glide_delay_ms: RwSignal<u16>,
) -> impl IntoView {
    let local_error = RwSignal::new(String::new());
    let is_busy = RwSignal::new(false);
    let identity_label = format!("{}  •  {}", monitor.backend, monitor.id);
    let monitor_error = monitor.error.clone();
    let monitor_error_when = monitor_error.clone();
    let title = monitor.label();
    let subtitle = monitor_subtitle(&monitor);
    let brightness_control = find_control(&monitor.controls, "10");
    let contrast_control = find_control(&monitor.controls, "12");
    let brightness_meter = brightness_control.clone();
    let contrast_meter = contrast_control.clone();
    let brightness_meter_when = brightness_control.clone();
    let contrast_meter_when = contrast_control.clone();
    let brightness_center = brightness_control.clone();
    let contrast_center = contrast_control.clone();
    let brightness_center_when = brightness_control.clone();
    let contrast_center_when = contrast_control.clone();
    let brightness_percent = brightness_control.as_ref().map(percent_fill).unwrap_or(0);
    let contrast_percent = contrast_control.as_ref().map(percent_fill).unwrap_or(0);
    let has_color_scenes = find_control(&monitor.controls, "16").is_some()
        && find_control(&monitor.controls, "18").is_some()
        && find_control(&monitor.controls, "1A").is_some();
    let mute_control = find_control(&monitor.controls, "8D");
    let mute_control_when = mute_control.clone();
    let power_control = find_control(&monitor.controls, "D6");
    let power_control_when = power_control.clone();
    let volume_control = find_control(&monitor.controls, "62");
    let volume_control_when = volume_control.clone();
    let toggle_controls: Vec<_> = monitor
        .controls
        .iter()
        .filter(|control| matches!(control.control_type, MonitorControlType::Toggle))
        .filter(|control| !matches!(control.code.as_str(), "8D" | "D6"))
        .cloned()
        .collect();
    let dock_controls: Vec<_> = monitor
        .controls
        .iter()
        .filter(|control| !matches!(control.code.as_str(), "10" | "12" | "14" | "62"))
        .filter(|control| !matches!(control.control_type, MonitorControlType::Toggle))
        .cloned()
        .collect();
    let preset_control = find_control(&monitor.controls, "14");
    let preset_control_when = preset_control.clone();
    let source_label = find_control(&monitor.controls, "60")
        .and_then(|control| selected_option_label(control.current_value, &control.options))
        .unwrap_or_else(|| String::from("DDC"));
    let preset_label = find_control(&monitor.controls, "14")
        .and_then(|control| selected_option_label(control.current_value, &control.options))
        .unwrap_or_else(|| String::from("Adaptive tone"));
    let preset_chip = preset_label.clone();
    let source_chip = source_label.clone();
    let source_signal = source_label.clone();
    let supports_text = if monitor.supports_controls() {
        "ready"
    } else {
        "limited"
    };
    let monitor_id_center = monitor.id.clone();
    let monitor_id_center_contrast = monitor.id.clone();
    let monitor_id_toggle = monitor.id.clone();
    let monitor_id_toggle_mute = monitor.id.clone();
    let monitor_id_volume = monitor.id.clone();
    let monitor_id_dock = monitor.id.clone();
    let monitor_id_preset_row = monitor.id.clone();
    let monitor_id_power = monitor.id.clone();

    view! {
        <div class="studio-plane">
            <header class="studio-header">
                <div class="studio-heading">
                    <p class="screen-kicker">{identity_label}</p>
                    <h2 class="screen-title">{title.clone()}</h2>
                    <p class="screen-subtitle">{subtitle.clone()}</p>
                </div>

                <div class="screen-badges">
                    <span class="badge">{supports_text}</span>
                    <span class="badge soft">{move || glide_label(glide_delay_ms.get())}</span>
                </div>
            </header>

            <section class="studio-layout">
                <aside class="inspector-column">
                    <div class="inspector-block">
                        <p class="panel-label">"Display link"</p>
                        <strong class="inspector-title">{title.clone()}</strong>
                        <p class="support-note">{subtitle.clone()}</p>
                    </div>

                    <div class="inspector-block compact">
                        <div class="chip-cluster">
                            <span class="hero-chip">{preset_chip}</span>
                            <span class="hero-chip muted">{source_chip}</span>
                            <span class="hero-chip muted">{monitor.backend.clone()}</span>
                        </div>
                    </div>

                    <div class="toggle-stack">
                        <Show when=move || mute_control_when.is_some()>
                            <ToggleControl
                                monitor_id=monitor_id_toggle_mute.clone()
                                monitors
                                control=mute_control.clone().unwrap()
                                is_busy
                                local_error
                            />
                        </Show>

                        <Show when=move || volume_control_when.is_some()>
                            <RangeControl
                                monitor_id=monitor_id_volume.clone()
                                monitors
                                control=volume_control.clone().unwrap()
                                glide_delay_ms
                                is_busy
                                local_error
                            />
                        </Show>

                        <For
                            each=move || toggle_controls.clone()
                            key=|control| control.code.clone()
                            children=move |control| {
                                view! {
                                    <ToggleControl
                                        monitor_id=monitor_id_toggle.clone()
                                        monitors
                                        control
                                        is_busy
                                        local_error
                                    />
                                }
                            }
                        />
                    </div>
                </aside>

                <section class="display-plane">
                    <div class="display-frame">
                        <div class="frame-accent top"></div>
                        <div class="frame-accent side"></div>

                        <div class="frame-head">
                            <div class="frame-copy">
                                <p class="monitor-kicker">"Signal surface"</p>
                                <h3>{title.clone()}</h3>
                                <p class="monitor-meta">
                                    "Current source, preset, and live DDC state for the selected display."
                                </p>
                            </div>

                            <Show when=move || power_control_when.is_some()>
                                <PowerModeSlider
                                    monitor_id=monitor_id_power.clone()
                                    monitors
                                    control=power_control.clone().unwrap()
                                    is_busy
                                    local_error
                                />
                            </Show>
                        </div>

                        <div class="signal-grid signal-grid-expanded">
                            <div class="signal-cell">
                                <span>"Brightness"</span>
                                <strong>{format!("{brightness_percent}%")}</strong>
                            </div>
                            <div class="signal-cell">
                                <span>"Contrast"</span>
                                <strong>{format!("{contrast_percent}%")}</strong>
                            </div>
                            <div class="signal-cell">
                                <span>"Input"</span>
                                <strong>{source_signal}</strong>
                            </div>
                        </div>

                        <div class="signal-rails">
                            <Show when=move || brightness_center_when.is_some()>
                                <CenterRangeControl
                                    monitor_id=monitor_id_center.clone()
                                    monitors
                                    control=brightness_center.clone().unwrap()
                                    glide_delay_ms
                                    local_error
                                    tone="warm"
                                />
                            </Show>
                            <Show when=move || contrast_center_when.is_some()>
                                <CenterRangeControl
                                    monitor_id=monitor_id_center_contrast.clone()
                                    monitors
                                    control=contrast_center.clone().unwrap()
                                    glide_delay_ms
                                    local_error
                                    tone="cool"
                                />
                            </Show>
                        </div>

                    </div>
                </section>

                <aside class="meter-stack">
                    <Show
                        when=move || brightness_meter_when.is_some()
                        fallback=|| view! { <div class="meter-placeholder"></div> }
                    >
                        <PrimaryRangeControl
                            control=brightness_meter.clone().unwrap()
                        />
                    </Show>

                    <Show
                        when=move || contrast_meter_when.is_some()
                        fallback=|| view! { <div class="meter-placeholder"></div> }
                    >
                        <PrimaryRangeControl
                            control=contrast_meter.clone().unwrap()
                        />
                    </Show>
                </aside>
            </section>

            <Show when=move || preset_control_when.is_some() || has_color_scenes>
                <section class="preset-row">
                    <PresetSceneControl
                        monitor_id=monitor_id_preset_row.clone()
                        monitors
                        preset_control=preset_control.clone()
                        has_color_scenes
                        is_busy
                        local_error
                    />
                </section>
            </Show>

            <section class="dock-strip">
                <For
                    each=move || dock_controls.clone()
                    key=|control| {
                        format!(
                            "{}-{}-{}",
                            control.code,
                            control.current_value.unwrap_or_default(),
                            control.supported
                        )
                    }
                    children=move |control| match control.control_type {
                        MonitorControlType::Range => view! {
                            <RangeControl
                                monitor_id=monitor_id_dock.clone()
                                monitors
                                control
                                glide_delay_ms
                                is_busy
                                local_error
                            />
                        }
                        .into_any(),
                        MonitorControlType::Toggle => ().into_any(),
                        MonitorControlType::Choice => view! {
                            <ChoiceControl
                                monitor_id=monitor_id_dock.clone()
                                monitors
                                control
                                is_busy
                                local_error
                                variant="dock"
                            />
                        }
                        .into_any(),
                    }
                />
            </section>

            <footer class="screen-footer">
                <Show when=move || !local_error.get().is_empty()>
                    <p class="support-note warning">{move || local_error.get()}</p>
                </Show>

                <Show when=move || monitor_error_when.is_some()>
                    <p class="support-note warning">
                        {monitor_error.clone().unwrap_or_default()}
                    </p>
                </Show>
            </footer>
        </div>
    }
}

#[component]
fn PresetSceneControl(
    monitor_id: String,
    monitors: RwSignal<Vec<MonitorSnapshot>>,
    preset_control: Option<MonitorControl>,
    has_color_scenes: bool,
    is_busy: RwSignal<bool>,
    local_error: RwSignal<String>,
) -> impl IntoView {
    let active_scene = RwSignal::new(None::<String>);
    let monitor_id_for_presets = StoredValue::new(monitor_id.clone());
    let monitor_id_for_scenes = StoredValue::new(monitor_id.clone());
    let preset_selected_value = RwSignal::new(
        preset_control
            .as_ref()
            .and_then(|control| control.current_value)
            .unwrap_or_default(),
    );
    let preset_options = preset_control
        .as_ref()
        .map(|control| control.options.clone())
        .unwrap_or_default();
    let preset_options_for_value = preset_options.clone();
    let preset_options_for_each = StoredValue::new(preset_options.clone());
    let preset_supported = preset_control
        .as_ref()
        .map(|control| control.supported)
        .unwrap_or(false);
    let preset_has_options = !preset_options.is_empty();
    let preset_error = preset_control
        .as_ref()
        .and_then(|control| control.error.clone());
    let preset_label = preset_control
        .as_ref()
        .map(|control| control.label.clone())
        .unwrap_or_else(|| String::from("Color Preset"));
    let preset_unavailable_message = preset_error
        .clone()
        .unwrap_or_else(|| format!("{preset_label} is unavailable."));
    let preset_no_values_message = format!("{preset_label} does not expose selectable values.");
    let preset_label_for_action = StoredValue::new(preset_label.clone());
    let preset_code = preset_control
        .as_ref()
        .map(|control| control.code.clone())
        .unwrap_or_else(|| String::from("14"));
    let preset_code_for_action = StoredValue::new(preset_code);
    let preset_unavailable_message_for_view = StoredValue::new(preset_unavailable_message);
    let preset_no_values_message_for_view = StoredValue::new(preset_no_values_message);

    view! {
        <section class="control-panel compact choice-panel dock preset-card combined-preset-card">
            <div class="control-header">
                <div>
                    <p class="panel-label">"Select Color Preset"</p>
                    <strong class="panel-value">
                        {move || option_label(&preset_options_for_value, preset_selected_value.get())}
                    </strong>
                </div>
            </div>

            <Show when=move || preset_control.is_some()>
                <div class="preset-row-group">
                    <div class="choice-strip compact" aria-label=preset_label.clone() role="group">
                        <For
                            each=move || preset_options_for_each.get_value()
                            key=|option| option.value
                            children=move |option| {
                                let option_value = option.value;
                                let option_label = option.label.clone();
                                let monitor_id = monitor_id_for_presets.get_value();
                                let control_code = preset_code_for_action.get_value();
                                let control_label = preset_label_for_action.get_value();

                                view! {
                                    <button
                                        class=move || {
                                            if preset_selected_value.get() == option_value {
                                                "choice-segment active"
                                            } else {
                                                "choice-segment"
                                            }
                                        }
                                        type="button"
                                        disabled=move || !preset_supported || is_busy.get() || !preset_has_options
                                        on:click=move |_| {
                                            if preset_selected_value.get_untracked() == option_value {
                                                return;
                                            }

                                            preset_selected_value.set(option_value);
                                            is_busy.set(true);
                                            local_error.set(String::new());

                                            let monitor_id = monitor_id.clone();
                                            let control_code = control_code.clone();
                                            let control_label = control_label.clone();

                                            spawn_local(async move {
                                                match interop::set_feature(&monitor_id, &control_code, option_value).await {
                                                    Ok(updated) => replace_monitor_snapshot(monitors, updated),
                                                    Err(error) => local_error.set(format!("{control_label}: {error}")),
                                                }

                                                is_busy.set(false);
                                            });
                                        }
                                    >
                                        {option_label}
                                    </button>
                                }
                            }
                        />
                    </div>

                    <Show when=move || !preset_supported || !preset_has_options>
                        <p class="support-note warning">
                            {if !preset_supported {
                                preset_unavailable_message_for_view.get_value()
                            } else {
                                preset_no_values_message_for_view.get_value()
                            }}
                        </p>
                    </Show>
                </div>
            </Show>

            <Show when=move || has_color_scenes>
                <div class="scene-row-group">
                    <div class="subcontrol-header">
                        <span class="panel-label">"Custom Scene"</span>
                        <strong class="panel-value">
                            {move || active_scene.get().unwrap_or_else(|| String::from("Manual gains"))}
                        </strong>
                    </div>

                    <div class="choice-strip compact" role="group" aria-label="Custom Scene">
                        <For
                            each=move || COLOR_SCENES.iter().copied()
                            key=|scene| scene.id
                            children=move |scene| {
                                let monitor_id = monitor_id_for_scenes.get_value();

                                view! {
                                    <button
                                        class=move || {
                                            if active_scene.get().as_deref() == Some(scene.id) {
                                                "choice-segment active"
                                            } else {
                                                "choice-segment"
                                            }
                                        }
                                        type="button"
                                        disabled=move || is_busy.get()
                                        on:click=move |_| {
                                            is_busy.set(true);
                                            local_error.set(String::new());
                                            active_scene.set(Some(scene.label.to_string()));

                                            let monitor_id = monitor_id.clone();
                                            spawn_local(async move {
                                                match interop::apply_color_scene(&monitor_id, scene.id).await {
                                                    Ok(updated) => replace_monitor_snapshot(monitors, updated),
                                                    Err(error) => local_error.set(format!("Custom Scene: {error}")),
                                                }

                                                is_busy.set(false);
                                            });
                                        }
                                    >
                                        {scene.label}
                                    </button>
                                }
                            }
                        />
                    </div>
                </div>
            </Show>
        </section>
    }
}

#[component]
fn PrimaryRangeControl(control: MonitorControl) -> impl IntoView {
    let max_value = control.max_value.unwrap_or(100);

    view! {
        <section class=move || {
            if control.supported {
                "meter-panel"
            } else {
                "meter-panel unsupported"
            }
        }>
            <p class="meter-label">{control.label.clone()}</p>
            <strong class="meter-number">{control.current_value.unwrap_or_default()}</strong>

            <div class="meter-legend">
                <span>
                    <em>"min"</em>
                    <strong>"0"</strong>
                </span>
                <span>
                    <em>"current"</em>
                    <strong>{control.current_value.unwrap_or_default()}</strong>
                </span>
                <span>
                    <em>"max"</em>
                    <strong>{max_value}</strong>
                </span>
            </div>

            <Show when=move || !control.supported>
                <p class="support-note warning">
                    {control.error.clone().unwrap_or_else(|| format!("{} is unavailable.", control.label))}
                </p>
            </Show>
        </section>
    }
}

#[component]
fn CenterRangeControl(
    monitor_id: String,
    monitors: RwSignal<Vec<MonitorSnapshot>>,
    control: MonitorControl,
    glide_delay_ms: RwSignal<u16>,
    local_error: RwSignal<String>,
    tone: &'static str,
) -> impl IntoView {
    let slider_value = RwSignal::new(control.current_value.unwrap_or_default());
    let range_change = RangeChangeContext {
        monitor_id: monitor_id.clone(),
        control_code: control.code.clone(),
        delay: glide_delay_ms.get_untracked() as u64,
        slider_value,
        pending_value: RwSignal::new(None::<u16>),
        in_flight: RwSignal::new(false),
        monitors,
        local_error,
        control_label: control.label.clone(),
    };
    let maximum = control.max_value.unwrap_or(100);

    let on_input = move |event: Event| {
        let input = event_target::<HtmlInputElement>(&event);
        if let Ok(parsed) = input.value().parse::<u16>() {
            slider_value.set(parsed);
        }
    };

    let on_change = move |event: Event| {
        let input = event_target::<HtmlInputElement>(&event);
        if let Ok(parsed) = input.value().parse::<u16>() {
            let mut next = range_change.clone();
            next.delay = glide_delay_ms.get() as u64;
            queue_range_change(next, parsed);
        }
    };

    view! {
        <div class="signal-rail-row interactive">
            <div class="signal-rail-head">
                <span>{control.label.clone()}</span>
                <strong>{move || slider_display(slider_value.get(), control.max_value)}</strong>
            </div>

            <div class=format!("signal-rail interactive {tone}")>
                <div
                    class=format!("signal-rail-fill {tone}")
                    style=move || format!(
                        "width: {}%;",
                        ((slider_value.get() as f32 / maximum.max(1) as f32) * 100.0).round() as u16
                    )
                ></div>
                <input
                    class="signal-slider"
                    type="range"
                    min="0"
                    max=maximum
                    prop:value=move || slider_value.get().to_string()
                    disabled=move || !control.supported
                    on:input=on_input
                    on:change=on_change
                />
            </div>

            <Show when=move || !control.supported>
                <p class="support-note warning">
                    {control.error.clone().unwrap_or_else(|| format!("{} is unavailable.", control.label))}
                </p>
            </Show>
        </div>
    }
}

#[component]
fn RangeControl(
    monitor_id: String,
    monitors: RwSignal<Vec<MonitorSnapshot>>,
    control: MonitorControl,
    glide_delay_ms: RwSignal<u16>,
    is_busy: RwSignal<bool>,
    local_error: RwSignal<String>,
) -> impl IntoView {
    let _ = is_busy;
    let slider_value = RwSignal::new(control.current_value.unwrap_or_default());
    let range_change = RangeChangeContext {
        monitor_id: monitor_id.clone(),
        control_code: control.code.clone(),
        delay: glide_delay_ms.get_untracked() as u64,
        slider_value,
        pending_value: RwSignal::new(None::<u16>),
        in_flight: RwSignal::new(false),
        monitors,
        local_error,
        control_label: control.label.clone(),
    };

    let on_input = move |event: Event| {
        let input = event_target::<HtmlInputElement>(&event);
        if let Ok(parsed) = input.value().parse::<u16>() {
            slider_value.set(parsed);
        }
    };

    let on_change = move |event: Event| {
        let input = event_target::<HtmlInputElement>(&event);
        if let Ok(parsed) = input.value().parse::<u16>() {
            let mut next = range_change.clone();
            next.delay = glide_delay_ms.get() as u64;
            queue_range_change(next, parsed);
        }
    };

    view! {
        <section class="control-panel range-panel">
            <div class="control-header">
                <div>
                    <p class="panel-label">{control.label.clone()}</p>
                    <strong class="panel-value">
                        {move || slider_display(slider_value.get(), control.max_value)}
                    </strong>
                </div>
                <span class="panel-tag">
                    {move || if glide_delay_ms.get() == 0 { "Instant" } else { "Ramped" }}
                </span>
            </div>

            <input
                class="slider"
                type="range"
                min="0"
                max=control.max_value.unwrap_or(100)
                prop:value=move || slider_value.get().to_string()
                style=move || format!(
                    "--slider-fill: {}%;",
                    control
                        .max_value
                        .map(|maximum| {
                            ((slider_value.get() as f32 / maximum.max(1) as f32) * 100.0).round()
                                as u16
                        })
                        .unwrap_or(0)
                )
                disabled=move || !control.supported
                on:input=on_input
                on:change=on_change
            />

            <Show when=move || !control.supported>
                <p class="support-note warning">
                    {control.error.clone().unwrap_or_else(|| format!("{} is unavailable.", control.label))}
                </p>
            </Show>
        </section>
    }
}

#[component]
fn ChoiceControl(
    monitor_id: String,
    monitors: RwSignal<Vec<MonitorSnapshot>>,
    control: MonitorControl,
    is_busy: RwSignal<bool>,
    local_error: RwSignal<String>,
    variant: &'static str,
) -> impl IntoView {
    let selected_value = RwSignal::new(control.current_value.unwrap_or_default());
    let control_code = control.code.clone();
    let control_label = control.label.clone();
    let options = control.options.clone();
    let options_for_value = options.clone();
    let has_options = !options.is_empty();
    let control_supported = control.supported;
    let control_error = control.error.clone();
    let card_variant = if control.code == "14" && variant == "dock" {
        "control-panel compact choice-panel dock preset-card"
    } else {
        "control-panel compact choice-panel"
    };

    view! {
        <section class=move || format!("{card_variant} {variant}")>
            <div class="control-header">
                <div>
                    <p class="panel-label">{control.label.clone()}</p>
                    <strong class="panel-value">
                        {move || option_label(&options_for_value, selected_value.get())}
                    </strong>
                </div>
            </div>

            <div class="choice-strip compact" aria-label=control.label.clone() role="group">
                <For
                    each=move || options.clone()
                    key=|option| option.value
                    children=move |option| {
                        let option_value = option.value;
                        let option_label = option.label.clone();
                        let monitor_id = monitor_id.clone();
                        let control_code = control_code.clone();
                        let control_label = control_label.clone();

                        view! {
                            <button
                                class=move || {
                                    if selected_value.get() == option_value {
                                        "choice-segment active"
                                    } else {
                                        "choice-segment"
                                    }
                                }
                                type="button"
                                disabled=move || !control_supported || is_busy.get() || !has_options
                                on:click=move |_| {
                                    if selected_value.get_untracked() == option_value {
                                        return;
                                    }

                                    let monitor_id = monitor_id.clone();
                                    let control_code = control_code.clone();
                                    let control_label = control_label.clone();
                                    selected_value.set(option_value);
                                    is_busy.set(true);
                                    local_error.set(String::new());

                                    spawn_local(async move {
                                        match interop::set_feature(&monitor_id, &control_code, option_value).await {
                                            Ok(updated) => replace_monitor_snapshot(monitors, updated),
                                            Err(error) => local_error.set(format!("{control_label}: {error}")),
                                        }

                                        is_busy.set(false);
                                    });
                                }
                            >
                                {option_label}
                            </button>
                        }
                    }
                />
            </div>

            <Show when=move || !control_supported || !has_options>
                <p class="support-note warning">
                    {if !control_supported {
                        control_error.clone().unwrap_or_else(|| format!("{} is unavailable.", control.label))
                    } else {
                        format!("{} does not expose selectable values.", control.label)
                    }}
                </p>
            </Show>
        </section>
    }
}

#[component]
fn ToggleControl(
    monitor_id: String,
    monitors: RwSignal<Vec<MonitorSnapshot>>,
    control: MonitorControl,
    is_busy: RwSignal<bool>,
    local_error: RwSignal<String>,
) -> impl IntoView {
    let options = control.options.clone();
    let has_toggle_options = options.len() >= 2;
    let off_option = options.first().cloned();
    let on_option = options.get(1).cloned();
    let control_supported = control.supported && has_toggle_options;
    let control_code = control.code.clone();
    let control_label = control.label.clone();
    let control_error = control.error.clone();
    let off_option_for_toggle = off_option.clone();
    let on_option_for_toggle = on_option.clone();
    let state = RwSignal::new(
        on_option
            .as_ref()
            .zip(control.current_value)
            .map(|(option, current)| current == option.value)
            .unwrap_or(false),
    );

    let on_toggle = move |_| {
        let Some(off_option) = off_option_for_toggle.clone() else {
            return;
        };
        let Some(on_option) = on_option_for_toggle.clone() else {
            return;
        };

        let monitor_id = monitor_id.clone();
        let control_code = control_code.clone();
        let control_label = control_label.clone();
        let next_on = !state.get();
        let next_value = if next_on {
            on_option.value
        } else {
            off_option.value
        };

        state.set(next_on);
        is_busy.set(true);
        local_error.set(String::new());

        spawn_local(async move {
            match interop::set_feature(&monitor_id, &control_code, next_value).await {
                Ok(updated) => replace_monitor_snapshot(monitors, updated),
                Err(error) => {
                    state.set(!next_on);
                    local_error.set(format!("{control_label}: {error}"));
                }
            }

            is_busy.set(false);
        });
    };

    view! {
        <section class="control-panel compact toggle-panel scene-toggle">
            <div class="control-header">
                <div>
                    <p class="panel-label">{control.label.clone()}</p>
                    <strong class="panel-value">
                        {move || {
                            if state.get() {
                                on_option
                                    .as_ref()
                                    .map(|option| option.label.clone())
                                    .unwrap_or_else(|| String::from("On"))
                            } else {
                                off_option
                                    .as_ref()
                                    .map(|option| option.label.clone())
                                    .unwrap_or_else(|| String::from("Off"))
                            }
                        }}
                    </strong>
                </div>

                <button
                    class=move || {
                        if state.get() {
                            "switch-control on"
                        } else {
                            "switch-control"
                        }
                    }
                    disabled=move || !control_supported || is_busy.get()
                    on:click=on_toggle
                    aria-label=control.label.clone()
                    aria-pressed=move || state.get()
                >
                    <span class="switch-track">
                        <span class="switch-state switch-state-off">"off"</span>
                        <span class="switch-state switch-state-on">"on"</span>
                        <span class="switch-thumb"></span>
                    </span>
                </button>
            </div>

            <Show when=move || !control_supported>
                <p class="support-note warning">
                    {control_error
                        .clone()
                        .unwrap_or_else(|| format!("{} is unavailable.", control.label))}
                </p>
            </Show>
        </section>
    }
}

#[component]
fn PowerModeSlider(
    monitor_id: String,
    monitors: RwSignal<Vec<MonitorSnapshot>>,
    control: MonitorControl,
    is_busy: RwSignal<bool>,
    local_error: RwSignal<String>,
) -> impl IntoView {
    let options = control.options.clone();
    let label = control.label.clone();
    let label_for_toggle = label.clone();
    let off_option = power_mode_option(&options, false);
    let on_option = power_mode_option(&options, true);
    let control_supported = control.supported && off_option.is_some() && on_option.is_some();
    let control_error = control.error.clone();
    let state = RwSignal::new(power_mode_is_on(control.current_value));

    let on_toggle = move |_| {
        let Some(off_option) = off_option.clone() else {
            return;
        };
        let Some(on_option) = on_option.clone() else {
            return;
        };

        let next_on = !state.get();
        let next_value = if next_on {
            on_option.value
        } else {
            off_option.value
        };

        if !next_on {
            let confirmed = window()
                .and_then(|win| {
                    win.confirm_with_message(
                        "Turn this monitor off? You may need to power it back on manually.",
                    )
                    .ok()
                })
                .unwrap_or(false);

            if !confirmed {
                return;
            }
        }

        let monitor_id = monitor_id.clone();
        let control_code = control.code.clone();
        let control_label = label_for_toggle.clone();

        state.set(next_on);
        is_busy.set(true);
        local_error.set(String::new());

        spawn_local(async move {
            match interop::set_feature(&monitor_id, &control_code, next_value).await {
                Ok(updated) => replace_monitor_snapshot(monitors, updated),
                Err(error) => {
                    state.set(!next_on);
                    local_error.set(format!("{control_label}: {error}"));
                }
            }

            is_busy.set(false);
        });
    };

    view! {
        <section class="power-slider-panel">
            <div class="power-slider-copy">
                <p class="panel-label">{label.clone()}</p>
                <strong>{move || if state.get() { "On" } else { "Off" }}</strong>
            </div>

            <button
                class=move || {
                    if state.get() {
                        "switch-control on power-switch"
                    } else {
                        "switch-control power-switch"
                    }
                }
                disabled=move || !control_supported || is_busy.get()
                on:click=on_toggle
                aria-label=label.clone()
                aria-pressed=move || state.get()
            >
                <span class="switch-track">
                    <span class="switch-state switch-state-off">"off"</span>
                    <span class="switch-state switch-state-on">"on"</span>
                    <span class="switch-thumb"></span>
                </span>
            </button>

            <Show when=move || !control_supported>
                <p class="support-note warning">
                    {control_error
                        .clone()
                        .unwrap_or_else(|| format!("{} is unavailable.", label))}
                </p>
            </Show>
        </section>
    }
}

fn glide_label(delay_ms: u16) -> String {
    match delay_ms {
        0 => String::from("Instant apply"),
        1..=10 => format!("{delay_ms} ms per step"),
        _ => format!("Soft ramp · {delay_ms} ms per step"),
    }
}

fn slider_display(current: u16, maximum: Option<u16>) -> String {
    match maximum {
        Some(maximum) if maximum > 0 => {
            let percent = ((current as f32 / maximum as f32) * 100.0).round() as u16;
            format!("{percent}%  ·  {current}/{maximum}")
        }
        _ => current.to_string(),
    }
}

fn find_control(controls: &[MonitorControl], code: &str) -> Option<MonitorControl> {
    controls
        .iter()
        .find(|control| control.code == code)
        .cloned()
}

fn selected_option_label(current_value: Option<u16>, options: &[ControlOption]) -> Option<String> {
    current_value.map(|value| option_label(options, value))
}

fn option_label(options: &[ControlOption], value: u16) -> String {
    options
        .iter()
        .find(|option| option.value == value)
        .map(|option| option.label.clone())
        .unwrap_or_else(|| format!("Value {value}"))
}

fn percent_fill(control: &MonitorControl) -> u16 {
    control
        .current_value
        .zip(control.max_value)
        .map(|(current, max)| ((current as f32 / max.max(1) as f32) * 100.0).round() as u16)
        .unwrap_or(0)
}

fn power_mode_option(options: &[ControlOption], want_on: bool) -> Option<ControlOption> {
    if want_on {
        options
            .iter()
            .find(|option| option.value == 0x01)
            .cloned()
            .or_else(|| options.iter().find(|option| option.value != 0x04).cloned())
    } else {
        options
            .iter()
            .find(|option| option.value == 0x04)
            .cloned()
            .or_else(|| options.iter().find(|option| option.value != 0x01).cloned())
    }
}

fn power_mode_is_on(current_value: Option<u16>) -> bool {
    current_value.map(|value| value != 0x04).unwrap_or(false)
}

fn monitor_subtitle(monitor: &MonitorSnapshot) -> String {
    match (&monitor.manufacturer_id, &monitor.serial_number) {
        (Some(mfg), Some(serial)) => format!("{mfg}  •  {serial}"),
        (Some(mfg), None) => mfg.clone(),
        (None, Some(serial)) => serial.clone(),
        (None, None) => String::new(),
    }
}

fn queue_range_change(ctx: RangeChangeContext, parsed: u16) {
    ctx.slider_value.set(parsed);
    ctx.pending_value.set(Some(parsed));
    ctx.local_error.set(String::new());

    if !ctx.in_flight.get_untracked() {
        drive_range_change(ctx);
    }
}

fn drive_range_change(ctx: RangeChangeContext) {
    let Some(target) = ctx.pending_value.get_untracked() else {
        return;
    };

    ctx.pending_value.set(None);
    ctx.in_flight.set(true);

    spawn_local(async move {
        let result = if ctx.delay == 0 {
            interop::set_feature(&ctx.monitor_id, &ctx.control_code, target).await
        } else {
            interop::transition_feature(&ctx.monitor_id, &ctx.control_code, target, ctx.delay).await
        };

        match result {
            Ok(updated) => {
                if ctx.pending_value.get_untracked().is_none() {
                    replace_monitor_snapshot(ctx.monitors, updated);
                }
            }
            Err(error) => ctx
                .local_error
                .set(format!("{}: {error}", ctx.control_label)),
        }

        ctx.in_flight.set(false);

        if ctx.pending_value.get_untracked().is_some() {
            drive_range_change(ctx);
        }
    });
}

fn replace_monitor_snapshot(monitors: RwSignal<Vec<MonitorSnapshot>>, updated: MonitorSnapshot) {
    monitors.update(|all| {
        if let Some(existing) = all.iter_mut().find(|monitor| monitor.id == updated.id) {
            *existing = updated;
        }
    });
}
