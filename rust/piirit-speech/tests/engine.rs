//! Engine plumbing against whisper.cpp's weightless test model (`models/for-tests-*`:
//! real shapes and vocabulary, no weights, so never any text): loads, runs, reports,
//! stops, fails as documented, on a thread of its own.
//!
//! Real model: `PIIRIT_SPEECH_MODEL=<ggml model>` runs `real_*` (skipped
//! unset; CI has no model).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use piirit_speech::{decode, Engine, Error, Options, Outcome, Progress, BUILT_LIBS, SAMPLE_RATE};

fn third_party(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/whisper.cpp")
        .join(path)
}

fn test_model() -> PathBuf {
    third_party("models/for-tests-ggml-tiny.bin")
}

fn libs() -> &'static Path {
    Path::new(BUILT_LIBS)
}

fn jfk() -> Vec<f32> {
    decode(&third_party("samples/jfk.wav"), 60).expect("decode jfk.wav")
}

/// Weightless model decodes every window to its token limit, 6 temperatures: seconds
/// each. Short clip keeps that cheap.
fn one_second() -> Vec<f32> {
    decode(&third_party("samples/jfk.wav"), 1).expect("decode jfk.wav")
}

/// `samples` repeated to `seconds`.
fn longer(samples: &[f32], seconds: usize) -> Vec<f32> {
    samples
        .iter()
        .copied()
        .cycle()
        .take(seconds * SAMPLE_RATE as usize)
        .collect()
}

fn keep_going(_: &Progress) -> bool {
    true
}

#[test]
fn a_run_reports_progress_to_the_end() {
    let mut engine = Engine::load(libs(), &test_model()).expect("load the test model");
    let mut seen = Vec::new();
    let outcome = engine
        .transcribe(&one_second(), &Options::default(), &mut |progress| {
            seen.push(progress.permille);
            true
        })
        .expect("transcribe");
    // Weightless model: whisper.cpp keeps no text from it.
    assert_eq!(outcome, Outcome::NoSpeech);
    assert_eq!(seen.first(), Some(&0), "{seen:?}");
    assert_eq!(seen.last(), Some(&1000), "{seen:?}");
    assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]), "{seen:?}");
}

#[test]
fn a_long_clip_is_reported_window_by_window() {
    let mut engine = Engine::load(libs(), &test_model()).expect("load the test model");
    let mut seen = Vec::new();
    let result = engine.transcribe(&longer(&jfk(), 45), &Options::default(), &mut |progress| {
        seen.push(progress.permille);
        // Second window reached: enough.
        progress.permille == 0
    });
    assert_eq!(result, Err(Error::Cancelled));
    // 45 s: windows start at 0 and 30 s.
    assert_eq!(seen, vec![0, 660]);
}

#[test]
fn a_false_from_progress_cancels() {
    let mut engine = Engine::load(libs(), &test_model()).expect("load the test model");
    let mut calls = 0;
    let result = engine.transcribe(&jfk(), &Options::default(), &mut |_| {
        calls += 1;
        false
    });
    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(calls, 1, "not asked again once told to stop");

    // Engine still good after: runs again.
    let mut started = false;
    let again = engine.transcribe(&one_second(), &Options::default(), &mut |_| {
        started = true;
        false
    });
    assert_eq!(again, Err(Error::Cancelled));
    assert!(started);
}

#[test]
fn languages_are_detected_chosen_or_given() {
    let mut engine = Engine::load(libs(), &test_model()).expect("load the test model");
    let speech = one_second();
    // Weightless model detects nothing in particular: chosen and under way, or reported.
    let several = Options {
        languages: vec!["fi".into(), "de".into(), "sv".into()],
        ..Options::default()
    };
    let outcome = engine.transcribe(&speech, &several, &mut |_| false);
    assert!(
        matches!(
            outcome,
            Err(Error::Cancelled) | Ok(Outcome::NoSpeech | Outcome::OtherLanguage { .. })
        ),
        "{outcome:?}"
    );
    // Given one: taken as said; stopped once under way.
    let one = Options {
        languages: vec!["en".into()],
        ..Options::default()
    };
    assert_eq!(
        engine.transcribe(&speech, &one, &mut |_| false),
        Err(Error::Cancelled)
    );

    let unknown = Options {
        languages: vec!["xx".into()],
        ..Options::default()
    };
    match engine.transcribe(&speech, &unknown, &mut keep_going) {
        Err(Error::Failed(why)) => assert!(why.contains("unknown language"), "{why}"),
        other => panic!("unknown language: {other:?}"),
    }
}

#[test]
fn too_little_sound_is_no_speech() {
    let mut engine = Engine::load(libs(), &test_model()).expect("load the test model");
    for samples in [Vec::new(), vec![0.0; 800]] {
        let outcome = engine.transcribe(&samples, &Options::default(), &mut keep_going);
        assert_eq!(outcome, Ok(Outcome::NoSpeech), "{} samples", samples.len());
    }
}

#[test]
fn a_bad_model_is_a_model_error() {
    let missing = Engine::load(libs(), Path::new("/nonexistent/ggml-small-q5_1.bin"));
    assert!(matches!(missing, Err(Error::Model(_))), "{missing:?}");

    let junk = std::env::temp_dir().join(format!("piirit-speech-junk-{}.bin", std::process::id()));
    std::fs::write(&junk, vec![7u8; 4096]).unwrap();
    let result = Engine::load(libs(), &junk);
    std::fs::remove_file(&junk).unwrap();
    match result {
        Err(Error::Model(why)) => assert!(!why.is_empty()),
        other => panic!("junk model: {other:?}"),
    }
}

#[test]
fn without_the_engine_library_the_engine_is_unavailable() {
    let empty = std::env::temp_dir().join(format!("piirit-speech-nolibs-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let result = Engine::load(&empty, &test_model());
    std::fs::remove_dir_all(&empty).unwrap();
    match result {
        Err(Error::Unavailable(why)) => assert!(why.contains("libpiirit-speech"), "{why}"),
        other => panic!("no library: {other:?}"),
    }
}

#[test]
fn an_engine_unloads_and_loads_again() {
    for _ in 0..2 {
        let mut engine = Engine::load(libs(), &test_model()).expect("load the test model");
        let mut started = false;
        let outcome = engine.transcribe(&one_second(), &Options::default(), &mut |_| {
            started = true;
            false
        });
        assert_eq!(outcome, Err(Error::Cancelled));
        assert!(started);
        drop(engine);
    }
}

/// Field `index` (1-based, after the `(comm)`) of `/proc/self/task/<tid>/stat`.
fn stat_field(stat: &str, index: usize) -> String {
    let after = &stat[stat.rfind(')').unwrap() + 2..];
    after.split(' ').nth(index - 3).unwrap().to_owned()
}

/// Nice value and allowed cores of a thread.
fn manners(task: &Path) -> (String, String) {
    let stat = std::fs::read_to_string(task.join("stat")).unwrap();
    let status = std::fs::read_to_string(task.join("status")).unwrap();
    let cpus = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .unwrap()
        .trim()
        .to_owned();
    (stat_field(&stat, 19), cpus)
}

/// Engine's threads: its own, and ggml's it starts (which inherit its name).
fn engine_threads() -> Vec<PathBuf> {
    std::fs::read_dir("/proc/self/task")
        .unwrap()
        .filter_map(Result::ok)
        .map(|task| task.path())
        .filter(|task| {
            std::fs::read_to_string(task.join("comm"))
                .is_ok_and(|name| name.trim() == "piirit-speech")
        })
        .collect()
}

#[test]
fn a_background_run_is_pinned_and_lowered_on_its_own_threads_only() {
    let caller = PathBuf::from("/proc/thread-self");
    let before = manners(&caller);
    let mut engine = Engine::load(libs(), &test_model()).expect("load the test model");
    // First progress: first window starting, after detection.
    let options = Options {
        languages: vec!["en".into()],
        cpus: vec![0],
        background: true,
    };
    let mut during = Vec::new();
    let result = engine.transcribe(&jfk(), &options, &mut |_| {
        during = engine_threads().iter().map(|task| manners(task)).collect();
        false
    });
    assert_eq!(result, Err(Error::Cancelled));
    assert!(!during.is_empty(), "no engine thread seen");
    for (nice, cpus) in &during {
        assert_eq!((nice.as_str(), cpus.as_str()), ("19", "0"));
    }
    assert_eq!(manners(&caller), before, "caller's thread left as it was");
}

/// Real model, if given: `PIIRIT_SPEECH_MODEL`.
fn real_model() -> Option<PathBuf> {
    std::env::var_os("PIIRIT_SPEECH_MODEL").map(PathBuf::from)
}

fn words(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn real_speech_is_transcribed() {
    let Some(model) = real_model() else {
        eprintln!("PIIRIT_SPEECH_MODEL unset: skipped");
        return;
    };
    let started = Instant::now();
    let mut engine = Engine::load(libs(), &model).expect("load the model");
    let loaded = started.elapsed();
    let speech = jfk();
    let mut partial = Vec::new();
    let started = Instant::now();
    let outcome = engine
        .transcribe(
            &speech,
            &Options {
                cpus: piirit_speech::big_cores(),
                ..Options::default()
            },
            &mut |progress| {
                partial.push(progress.clone());
                true
            },
        )
        .expect("transcribe");
    let took = started.elapsed();
    eprintln!("load {loaded:?}, transcribe {took:?}: {outcome:?}");
    let Outcome::Text { language, text } = outcome else {
        panic!("no text");
    };
    assert_eq!(language, "en");
    assert!(
        words(&text).contains("ask not what your country can do for you"),
        "{text}"
    );
    assert!(partial.iter().any(|progress| !progress.text.is_empty()));
}

#[test]
fn real_speech_in_an_unselected_language_is_reported_not_transcribed() {
    let Some(model) = real_model() else {
        eprintln!("PIIRIT_SPEECH_MODEL unset: skipped");
        return;
    };
    let mut engine = Engine::load(libs(), &model).expect("load the model");
    let speech = jfk();
    let options = |languages: &[&str]| Options {
        languages: languages.iter().map(|&code| code.to_owned()).collect(),
        ..Options::default()
    };
    let other = engine
        .transcribe(&speech, &options(&["fi", "de"]), &mut keep_going)
        .expect("detect");
    assert_eq!(
        other,
        Outcome::OtherLanguage {
            language: "en".into()
        }
    );
    let chosen = engine
        .transcribe(&speech, &options(&["fi", "en"]), &mut keep_going)
        .expect("transcribe");
    assert!(
        matches!(&chosen, Outcome::Text { language, .. } if language == "en"),
        "{chosen:?}"
    );
}

#[test]
fn real_silence_and_hiss_are_no_speech_whatever_the_languages() {
    let Some(model) = real_model() else {
        eprintln!("PIIRIT_SPEECH_MODEL unset: skipped");
        return;
    };
    let mut engine = Engine::load(libs(), &model).expect("load the model");
    let silence = vec![0.0; 5 * SAMPLE_RATE as usize];
    // Quiet noise, about -55 dBFS: a phone in a pocket.
    let mut seed = 1u32;
    let hiss: Vec<f32> = silence
        .iter()
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            f32::from(u16::try_from(seed >> 16).unwrap()) / 32_768.0 * 0.003 - 0.003
        })
        .collect();
    for sound in [&silence, &hiss] {
        for languages in [
            vec![],
            vec!["en".to_owned()],
            vec!["en".into(), "fi".into()],
        ] {
            let options = Options {
                languages,
                ..Options::default()
            };
            let outcome = engine
                .transcribe(sound, &options, &mut keep_going)
                .expect("transcribe");
            // Whisper alone says "Thank you." or "you" here.
            assert_eq!(outcome, Outcome::NoSpeech, "{:?}", options.languages);
        }
    }
}
