//! Vendored `qmetaobject::single_shot` keeps a wait's sub-second part (0.2.10 dropped it:
//! `third_party/qmetaobject.patch`). Order asserted, not clock: Qt fires due timers by deadline,
//! however late the loop gets to them.

// Qt harness: `unsafe` `set_var` pre-Qt; this file tests `single_shot` itself.
#![allow(unsafe_code, unused_unsafe, clippy::disallowed_methods)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use qmetaobject::*;

#[test]
fn a_wait_keeps_its_fraction_and_a_huge_one_saturates() {
    // SAFETY: single-threaded; platform set before Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let engine = QmlEngine::new();
    let engine_ptr: *const QmlEngine = &engine;
    let fired: Rc<RefCell<Vec<&str>>> = Rc::default();

    // Wraps to 100 ms if cast to `int` unclamped; saturated, ~24 days.
    let huge = Duration::from_millis(u64::from(u32::MAX) + 101);
    // 1.5 s registered before 1 s: fraction dropped, both due at 1 s, 1.5 s runs first.
    for (label, wait) in [
        ("huge", huge),
        ("1.5 s", Duration::from_millis(1500)),
        ("1 s", Duration::from_secs(1)),
    ] {
        let fired = fired.clone();
        single_shot(wait, move || {
            fired.borrow_mut().push(label);
            if label == "1.5 s" {
                // SAFETY: fires only in `exec()`; `engine` outlives it.
                unsafe { (*engine_ptr).quit() };
            }
        });
    }

    engine.exec();

    assert_eq!(
        *fired.borrow(),
        ["1 s", "1.5 s"],
        "1.5 s fired with or before 1 s: the half second was dropped; or the huge wait fired"
    );
}
