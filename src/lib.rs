extern crate fringe;
extern crate futures;
use fringe::OsStack;
use fringe::generator::{Generator, Yielder};
use futures::{Future, Poll, Async};

#[derive(Debug)]
pub struct FringeFut<T: Send, E: Send> {
    gen: Generator<(), Async<Result<T, E>>, OsStack>,
}

#[derive(Debug, Clone)]
pub struct SchedThunk<'a, T: Send + 'a, E: Send + 'a>(&'a Yielder<(), Async<Result<T, E>>>);
const STACKSZ: usize = 1 << 20;

// impl<'a, Input, Output, Stack> Generator<'a, Input, Output, Stack> where Input: 'a, Output: 'a, Stack: Stack
// fn resume(&mut self, input: Input) -> Option<Output>
impl<T: Send, E: Send> FringeFut<T, E> {
    pub fn new<F>(f: F) -> Self
        where F: FnOnce(SchedThunk<T, E>) -> Result<T, E> + Send
    {
        let stack = OsStack::new(STACKSZ).expect("OsStack::new");
        let mut gen = {
            Generator::new(stack, move |yielder, mut _in: ()| {
                let wr = SchedThunk(yielder);
                let res = f(wr);
                yielder.suspend(Async::Ready(res));
            })
        };
        FringeFut { gen: gen }
    }
}
impl<'a, T: Send + 'a, E: Send + 'a> SchedThunk<'a, T, E> {
    pub fn await<F: Future>(&self, mut f: F) -> Result<F::Item, F::Error> {
        loop {
            match f.poll() {
                Ok(Async::NotReady) => self.0.suspend(Async::NotReady),
                Ok(Async::Ready(val)) => return Ok(val),
                Err(e) => return Err(e),
            }
        }
    }
}

impl<T: Send, E: Send> Future for FringeFut<T, E> {
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.gen.resume(()) {
            Some(Async::NotReady) => Ok(Async::NotReady),
            Some(Async::Ready(Ok(r))) => Ok(Async::Ready(r)),
            Some(Async::Ready(Err(e))) => Err(e),
            None => {
                panic!("Future generator exited");
            }
        }
    }
}

#[cfg(test)]
mod test {
    extern crate tokio_timer;
    use self::tokio_timer::Timer;
    use std::time::Duration;
    use super::FringeFut;
    use futures::Future;

    #[test]
    fn smoketest() {
        let timer = Timer::default();

        let f = FringeFut::<usize, ()>::new(|yielder| {
            for n in 0..5 {
                let dur = Duration::from_millis(200);
                println!("n:{:?}", dur);
                yielder.await(timer.sleep(dur)).expect("await");
            }
            Ok(42usize)
        });

        println!("Before:{:?}", f);
        let res = f.wait();
        println!("res:{:?}", res);
    }
}
