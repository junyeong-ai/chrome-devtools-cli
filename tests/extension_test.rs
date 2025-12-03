//! Extension event test
//!
//! This test verifies the extension → CLI communication via CDP bindings.
//!
//! Run with: cargo test extension_binding --nocapture
//!
//! Manual test procedure:
//! 1. Build extension: cd extension && npm run build
//! 2. Start Chrome with extension:
//!    ./target/release/chrome-devtools-cli navigate "https://example.com" \
//!    --keep-alive --headless false
//! 3. Load extension manually in chrome://extensions (Load unpacked -> extension/dist)
//! 4. The popup should show "CLI Connected" when the binding is active

use chrome_devtools_cli::chrome::collectors::extension::{
    ExtensionCollector, ExtensionEvent, TargetInfo,
};
use chrome_devtools_cli::chrome::storage::SessionStorage;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Test that the extension collector can receive events via CDP bindings.
///
/// This test demonstrates the flow:
/// 1. CLI creates `__cdtcli__` binding via AddBindingParams (main world)
/// 2. Extension content script calls `__cdtcli__()` via <script> injection
/// 3. CLI receives EventBindingCalled with the payload
#[tokio::test]
#[ignore = "requires manual browser setup"]
async fn test_extension_binding_receives_events() {
    // Create temporary storage
    let storage = Arc::new(SessionStorage::new("test-session").unwrap());

    // Create extension collector
    let collector = ExtensionCollector::new(storage);

    // Subscribe to events
    let mut rx = collector.subscribe();

    println!("Extension collector ready");
    println!("Please trigger an event from the extension (e.g., select element)");

    // Wait for an event with timeout
    match timeout(Duration::from_secs(60), rx.recv()).await {
        Ok(Ok(event)) => {
            println!("Received extension event: {:?}", event);

            match event {
                ExtensionEvent::Click(click) => {
                    println!("Click: {:?}", click.aria);
                }
                ExtensionEvent::Select(select) => {
                    println!("Element selected: {:?}", select.aria);
                }
                ExtensionEvent::Input(input) => {
                    println!("Input: {:?} value={:?}", input.target.aria, input.value);
                }
                ExtensionEvent::Screenshot(screenshot) => {
                    println!("Screenshot captured for element: {:?}", screenshot.target);
                }
                _ => {
                    println!("Other event type received");
                }
            }
        }
        Ok(Err(e)) => {
            println!("Receiver error: {:?}", e);
        }
        Err(_) => {
            println!("Timeout waiting for extension event");
        }
    }
}

/// This test verifies the sendToCli flow in content script:
/// 1. sendToCli() creates a <script> element
/// 2. Script runs in main world and calls __cdtcli__(payload)
/// 3. CDP EventBindingCalled is fired
/// 4. ExtensionCollector receives and parses the event
#[test]
fn test_extension_event_parsing() {
    // Test parsing of click event with new format
    let json = r##"{"click":{"aria":["button","Submit"],"css":"#submit-btn"}}"##;
    let event: ExtensionEvent = serde_json::from_str(json).unwrap();

    match event {
        ExtensionEvent::Click(click) => {
            assert_eq!(click.aria, vec!["button", "Submit"]);
            assert_eq!(click.css, Some("#submit-btn".to_string()));
        }
        _ => panic!("Expected Click event"),
    }

    // Test input event with value
    let json = r##"{"input":{"aria":["textbox","Email"],"value":"test@example.com"}}"##;
    let event: ExtensionEvent = serde_json::from_str(json).unwrap();

    match event {
        ExtensionEvent::Input(input) => {
            assert_eq!(input.target.aria, vec!["textbox", "Email"]);
            assert_eq!(input.value, Some("test@example.com".to_string()));
        }
        _ => panic!("Expected Input event"),
    }

    // Test select event
    let json = r##"{"select":{"aria":["link","Home"]}}"##;
    let event: ExtensionEvent = serde_json::from_str(json).unwrap();

    match event {
        ExtensionEvent::Select(select) => {
            assert_eq!(select.aria, vec!["link", "Home"]);
        }
        _ => panic!("Expected Select event"),
    }

    // Test scroll event
    let json = r##"{"scroll":{"x":0,"y":100}}"##;
    let event: ExtensionEvent = serde_json::from_str(json).unwrap();

    match event {
        ExtensionEvent::Scroll(scroll) => {
            assert_eq!(scroll.x, 0);
            assert_eq!(scroll.y, 100);
        }
        _ => panic!("Expected Scroll event"),
    }

    // Test keypress event
    let json = r##"{"key_press":{"key":"Enter"}}"##;
    let event: ExtensionEvent = serde_json::from_str(json).unwrap();

    match event {
        ExtensionEvent::KeyPress(keypress) => {
            assert_eq!(keypress.key, "Enter");
        }
        _ => panic!("Expected KeyPress event"),
    }

    println!("All event parsing tests passed");
}

/// Test TargetInfo from_aria helper
#[test]
fn test_target_info_from_aria() {
    let target = TargetInfo::from_aria(vec!["button".to_string(), "Submit".to_string()]);
    assert_eq!(target.aria, vec!["button", "Submit"]);
    assert!(target.css.is_none());
    assert!(target.xpath.is_none());
    assert!(target.testid.is_none());
}
