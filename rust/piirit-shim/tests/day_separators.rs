//! Which day a message belongs to, across a daylight-saving change.
//!
//! Each timestamp is read in the offset that was in force at its own
//! instant. Adding the offset in force *now* to every timestamp is right
//! for most of the year and wrong across a DST boundary, where a message
//! within an hour of local midnight sits under the wrong heading and moves
//! as the year turns.

// `set_var` before the first timezone lookup; only unsafe from edition
// 2024 on, hence `unused_unsafe`.
#![allow(unsafe_code, unused_unsafe)]

use piirit_shim::local_day_number;

/// Days since the epoch for a date, the same count the model produces.
fn day(year: i32, month: u32, of_month: u32) -> i64 {
    // Kept deliberately dumb: a second implementation of the thing under
    // test would just repeat its mistakes.
    let days_in = |y: i32, m: u32| -> i64 {
        match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
            _ => 28,
        }
    };
    let mut total = 0;
    for y in 1970..year {
        total += if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
            366
        } else {
            365
        };
    }
    for m in 1..month {
        total += days_in(year, m);
    }
    total + i64::from(of_month) - 1
}

#[test]
fn a_message_near_midnight_lands_on_its_own_local_day_either_side_of_dst() {
    // SAFETY: set before anything asks for the zone, in a single-threaded
    // test binary.
    unsafe {
        std::env::set_var("TZ", "Europe/Berlin");
    }

    // Berlin is UTC+2 in July and UTC+1 in January. Both instants are
    // within an hour of local midnight, which is where an hour of error
    // shows up as a whole day.
    //
    // 22:30Z on 15 July is 00:30 on the 16th in Berlin.
    let summer = 1_784_154_600;
    // 22:30Z on 15 January is 23:30 on the 15th in Berlin.
    let winter = 1_768_516_200;

    let summer_day = local_day_number(summer);
    let winter_day = local_day_number(winter);

    assert_eq!(
        summer_day,
        day(2026, 7, 16),
        "a message sent just after local midnight in summer was filed under \
         the previous day (got {summer_day}, wanted {})",
        day(2026, 7, 16)
    );
    assert_eq!(
        winter_day,
        day(2026, 1, 15),
        "a message sent just before local midnight in winter was filed under \
         the next day (got {winter_day}, wanted {})",
        day(2026, 1, 15)
    );

    // The point of having both: no single fixed offset satisfies them.
    // +1h files the summer message a day early, +2h files the winter one a
    // day late, which is precisely what the old code did depending on the
    // month it happened to be run in.
    for offset in [3600_i64, 7200] {
        let naive_summer = (summer + offset).div_euclid(86_400);
        let naive_winter = (winter + offset).div_euclid(86_400);
        assert!(
            naive_summer != summer_day || naive_winter != winter_day,
            "a fixed offset of {offset}s got both days right, so this test \
             no longer distinguishes the bug it was written for"
        );
    }
}
