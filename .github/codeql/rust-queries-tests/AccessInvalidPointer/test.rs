// Real dangling derefs must stay reported; derive(QObject)'s own derefs must not.
use qmetaobject::*;
use std::ptr;

#[derive(QObject, Default)]
pub struct Widget {
    base: qt_base_class!(trait QObject),
    pub count: qt_property!(u32; NOTIFY count_changed),
    pub count_changed: qt_signal!(),
    pub poke: qt_method!(fn(&self) -> i64),
}

impl Widget {
    // Hand-written method of a QObject: same rule applies.
    pub fn poke(&self) -> i64 {
        let p: *const i64 = ptr::null();
        unsafe { *p } // BAD: reported
    }
}

pub fn freed() -> i64 {
    let mut b = Box::new(1i64);
    let p: *mut i64 = &mut *b;
    std::mem::forget(b);
    unsafe {
        ptr::drop_in_place(p);
        *p // BAD: reported
    }
}

// Macro, but not derive(QObject): still reported.
macro_rules! read_null {
    () => {
        unsafe { *ptr::null::<i64>() } // BAD: reported
    };
}

pub fn via_macro() -> i64 {
    read_null!()
}

fn main() {}
