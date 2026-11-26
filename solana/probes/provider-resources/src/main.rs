use musical_chairs_provider_resources::probe_snapshot;

fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&probe_snapshot()).expect("probe snapshot JSON")
    );
}
