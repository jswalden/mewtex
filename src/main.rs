use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut, Drop};
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rand::Rng;

struct MewtexGuard<'a, T> {
    mewtex: &'a Mewtex<T>,
}

unsafe impl<'a, T> Send for MewtexGuard<'a, T> where T: Send {}

impl<'a, T> Deref for MewtexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let ucell = &self.mewtex.value;
        // Safety: Access to a MewtexGuard is extended from a locked Mewtex.
        unsafe { &*ucell.get() }
    }
}
impl<'a, T> DerefMut for MewtexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let ucell = &self.mewtex.value;
        // Safety: Access to a MewtexGuard is extended from a locked Mewtex.
        unsafe { &mut *ucell.get() }
    }
}

impl<'a, T> Drop for MewtexGuard<'a, T> {
    fn drop(&mut self) {
        let locked = &self.mewtex.state;

        // This pairs with the Acquire in Mewtex::lock.
        locked.store(UNLOCKED, Ordering::Release);
        atomic_wait::wake_one(locked);
    }
}

impl<'a, T> MewtexGuard<'a, T> {
    fn new(mewtex: &Mewtex<T>) -> MewtexGuard<T> {
        MewtexGuard { mewtex }
    }
}

struct Mewtex<T> {
    state: AtomicU32,
    value: UnsafeCell<T>,
}

unsafe impl<T> Sync for Mewtex<T> {}
unsafe impl<T> Send for Mewtex<T> {}

const UNLOCKED: u32 = 0;
const LOCKED: u32 = 1;

impl<T> Mewtex<T> {
    fn new(t: T) -> Mewtex<T> {
        Mewtex {
            state: AtomicU32::new(UNLOCKED),
            value: UnsafeCell::new(t),
        }
    }

    fn lock(&self) -> MewtexGuard<T> {
        loop {
            if let Err(_) = self.state.compare_exchange_weak(
                UNLOCKED,
                LOCKED,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                atomic_wait::wait(&self.state, LOCKED);
            } else {
                break;
            }
        }

        MewtexGuard::new(self)
    }

    fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }
}

fn main() {
    let str_vec = Arc::new(Mewtex::new(vec![]));

    const NUM_REFS: usize = 8;

    let mut ts = vec![];
    for i in 0..NUM_REFS {
        let arcref = str_vec.clone();
        let t = thread::spawn(move || {
            thread::sleep(Duration::from_millis(rand::thread_rng().gen_range(0..64)));

            let mut g = arcref.lock();
            g.push(format!("thread {}", i));
        });

        ts.push(t);
    }

    {
        let mut g = str_vec.lock();

        g.push("main".to_string());
    }

    for t in ts {
        t.join().expect("no panic");
    }

    {
        let str_vec = str_vec.lock();
        let str_vec = &*str_vec;

        for s in str_vec {
            println!("Received: {}", s);
        }
    }
}
