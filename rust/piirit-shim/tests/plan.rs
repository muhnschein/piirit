//! `common::Plan`, which every Qt test's steps run on. Step waits counted from end of one before,
//! not from start: an overrunning step can't pull next in right behind it. `until` stops at first
//! done; only last try told it is last.

// Qt harness: `unsafe` `set_var` pre-Qt; `borrow_as_ptr`.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use qmetaobject::*;

mod common;

#[test]
fn a_step_that_runs_long_still_gives_the_next_its_whole_wait() {
    // SAFETY: single-threaded; platform set before Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let engine = QmlEngine::new();
    let engine_ptr: *const QmlEngine = &engine;
    let ended = Rc::new(RefCell::new(None::<Instant>));
    let gap = Rc::new(RefCell::new(None::<Duration>));
    let done_at = Rc::new(RefCell::new(Vec::new()));
    let never_done = Rc::new(RefCell::new(Vec::new()));

    let mut plan = common::Plan::default();

    // Overruns its own wait and next one's: from-start times would run next at once.
    let mark = ended.clone();
    plan.after(0, move || {
        std::thread::sleep(Duration::from_millis(1500));
        *mark.borrow_mut() = Some(Instant::now());
    });

    let (mark, measured) = (ended, gap.clone());
    plan.after(1, move || {
        *measured.borrow_mut() = mark.borrow().map(|at| at.elapsed());
    });

    // Done on third try: two more never asked.
    let (seen, mut asked) = (done_at.clone(), 0);
    plan.until(0, 5, move |last| {
        seen.borrow_mut().push(last);
        asked += 1;
        asked == 3
    });

    // Never done: every try, last one told so; then plan goes on.
    let seen = never_done.clone();
    plan.until(0, 3, move |last| {
        seen.borrow_mut().push(last);
        false
    });

    plan.after(0, move || {
        // SAFETY: fires only in `exec()`; `engine` outlives it.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    plan.run();
    engine.exec();

    // Lower bound only: load makes gap longer, never shorter. Qt's coarse timers may fire up to 5%
    // early.
    let gap = gap
        .borrow()
        .expect("the step after the long one did not run");
    assert!(
        gap >= Duration::from_millis(900),
        "next step ran {gap:?} after the long one ended, not its whole second"
    );
    assert_eq!(
        *done_at.borrow(),
        [false, false, false],
        "a poll did not stop at its first done"
    );
    assert_eq!(
        *never_done.borrow(),
        [false, false, true],
        "a poll that never got done was not asked each try, last one told so"
    );
}
