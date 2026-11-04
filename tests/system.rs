#![cfg(not(miri))]

//! End-to-end test against the real LV2 bundles installed on the system.
//!
//! Skipped gracefully when the LV2 example amp is not installed.

use maolan_lv2::World;
use maolan_lv2::raw::LV2Feature;

const EG_AMP_URI: &str = "http://lv2plug.in/plugins/eg-amp";
const EG_AMP_BUNDLE: &str = "/usr/local/lib/lv2/eg-amp.lv2/manifest.ttl";

#[test]
fn discover_and_run_eg_amp() {
    if !std::path::Path::new(EG_AMP_BUNDLE).exists() {
        eprintln!("skipping: {EG_AMP_BUNDLE} not installed");
        return;
    }

    let world = World::new();
    world.load_all();

    let uri = world.new_uri(EG_AMP_URI);
    let plugin = world
        .plugins()
        .plugin(&uri)
        .unwrap_or_else(|| panic!("plugin not found: {EG_AMP_URI}"));
    assert!(plugin.verify());
    assert_eq!(plugin.name().as_str(), Some("Simple Amplifier"));
    assert_eq!(
        plugin.class().label().as_str(),
        Some("Amplifier Plugin"),
        "class label should come from the loaded LV2 spec data"
    );
    assert!(
        plugin
            .bundle_uri()
            .as_uri()
            .is_some_and(|u| u.contains("/eg-amp.lv2/"))
    );
    assert_eq!(plugin.ports_count(), 3);

    // Instantiate with an empty feature list (eg-amp requires none).
    let features: Vec<&LV2Feature> = Vec::new();
    let mut instance =
        unsafe { plugin.instantiate(48000.0, features) }.expect("failed to instantiate eg-amp");
    assert_eq!(instance.uri(), Some(EG_AMP_URI));
    assert!(instance.descriptor().is_some());
    assert!(!instance.handle().is_null());

    // Port layout: 0 = gain (control, dB), 1 = in (audio), 2 = out (audio).
    const FRAMES: usize = 512;
    let mut gain = 0.0_f32; // 0 dB -> unity gain
    let mut input = [1.0_f32; FRAMES];
    let mut output = [0.0_f32; FRAMES];
    unsafe {
        instance.connect_port_mut(0, &raw mut gain);
        instance.connect_port_mut(1, input.as_mut_ptr());
        instance.connect_port_mut(2, output.as_mut_ptr());
    }

    let mut active = unsafe { instance.activate() };
    unsafe { active.run(FRAMES) };
    assert!(
        output.iter().all(|&s| s == 1.0),
        "0 dB gain must copy input to output, got {:?}",
        &output[..4]
    );

    // +20 dB -> amplitude factor 10 (eg-amp uses 10^(dB/20)).
    gain = 20.0;
    input = [0.1_f32; FRAMES];
    output = [0.0_f32; FRAMES];
    unsafe {
        active.instance_mut().connect_port_mut(0, &raw mut gain);
        active
            .instance_mut()
            .connect_port_mut(1, input.as_mut_ptr());
        active
            .instance_mut()
            .connect_port_mut(2, output.as_mut_ptr());
        active.run(FRAMES);
    }
    assert!(
        output.iter().all(|&s| (s - 1.0).abs() < 1e-5),
        "+20 dB on 0.1 input must give 1.0, got {:?}",
        &output[..4]
    );

    // eg-amp provides no extension data.
    assert!(
        unsafe {
            active
                .instance()
                .extension_data::<std::ffi::c_void>("http://lv2plug.in/ns/ext/state#interface")
        }
        .is_none()
    );

    let instance = unsafe { active.deactivate() }.expect("deactivate");
    drop(instance);
}

#[test]
fn load_all_finds_bundles() {
    if !std::path::Path::new(EG_AMP_BUNDLE).exists() {
        eprintln!("skipping: {EG_AMP_BUNDLE} not installed");
        return;
    }
    let world = World::new();
    world.load_all();
    // The spec bundles ship no plugins, but the example plugins and whatever
    // is installed system-wide must show up; at minimum the example amp.
    assert!(world.plugins().count() >= 1);
    assert!(world.plugins().plugin(&world.new_uri(EG_AMP_URI)).is_some());
}
