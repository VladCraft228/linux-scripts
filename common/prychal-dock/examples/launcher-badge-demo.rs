//! Optional manual check: a persistent publisher, so its badge survives until Enter.
use std::{collections::HashMap, error::Error, io};
use zbus::{blocking::Connection, zvariant::OwnedValue};
fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let id = args.next().ok_or("Usage: launcher-badge-demo DESKTOP_ID [COUNT|urgent]")?;
    if id.contains('/') || id.contains('\\') { return Err("Use a desktop ID, not a file path".into()); }
    let mode = args.next().unwrap_or_else(|| "5".into());
    let urgent = mode == "urgent";
    let count = if urgent { 0 } else { mode.parse::<i64>()?.clamp(0, 9999) };
    let id = if id.ends_with(".desktop") { id } else { format!("{id}.desktop") };
    let uri = format!("application://{id}");
    let connection = Connection::session()?;
    let send = |count: i64, visible: bool, urgent: bool| -> zbus::Result<()> {
        let properties = HashMap::from([
            ("count".to_string(), OwnedValue::from(count)),
            ("count-visible".to_string(), OwnedValue::from(visible)),
            ("urgent".to_string(), OwnedValue::from(urgent)),
        ]);
        connection.emit_signal(None::<&str>, "/com/canonical/unity/launcherentry/dock_demo", "com.canonical.Unity.LauncherEntry", "Update", &(uri.as_str(), properties))
    };
    send(count, !urgent, urgent)?;
    println!("Badge sent for {id}. Keep this terminal open. Press Enter to clear and quit.");
    io::stdin().read_line(&mut String::new())?;
    send(0, false, false)?;
    Ok(())
}
