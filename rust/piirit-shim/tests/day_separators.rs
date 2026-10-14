//! Message's day across DST change.
//!
//! Offset in force at each instant. Offset "now" misfiles messages near midnight across DST.

// `set_var` before tz lookup. Unsafe only from edition 2024: `unused_unsafe`.
#![allow(unsafe_code, unused_unsafe)]

use piirit_shim::local_day_number;

/// Days since epoch. Same count as model.
fn day(year: i32, month: u32, of_month: u32) -> i64 {
    // Dumb on purpose: a second impl of code under test would repeat its mistakes.
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
    // SAFETY: set before zone read; single-threaded.
    unsafe {
        std::env::set_var("TZ", "Europe/Berlin");
    }

    // Berlin: UTC+2 July, UTC+1 January. Near midnight, an hour of error is a day.
    //
    // 22:30Z 15 Jul = 00:30 16 Jul.
    let summer = 1_784_154_600;
    // 22:30Z 15 Jan = 23:30 15 Jan.
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

    // No fixed offset passes both: +1h files summer early, +2h winter late (old code, by month).
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
