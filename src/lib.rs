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
        let gen = {
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
    use std::time::Duration;
    use std::mem;
    use super::FringeFut;
    use futures::{Future, Poll, Async, task};
    use std::sync::{Arc, atomic};
    #[derive(Debug)]
    enum CanaryFuture {
        Ready(usize),
        Go(usize),
        Done,
    }

    impl Future for CanaryFuture {
        type Item = usize;
        type Error = ();
        fn poll(&mut self) -> Poll<usize, ()> {
            println!("CanaryFuture#poll:{:?}", self);
            match mem::replace(self, CanaryFuture::Done) {
                CanaryFuture::Ready(n) => {
                    *self = CanaryFuture::Go(n);
                    task::park().unpark();
                    Ok(Async::NotReady)
                }
                CanaryFuture::Go(n) => Ok(Async::Ready(n)),
                CanaryFuture::Done => unreachable!(),
            }
        }
    }

    #[test]
    fn smoketest() {
        let f = FringeFut::<usize, ()>::new(|yielder| {
            for n in 0..5 {
                let r = yielder.await(CanaryFuture::Ready(n)).expect("await");
                println!("a:{:?}", r);
            }
            Ok(42usize)
        });

        println!("Before:{:?}", f);
        // Helpfully seems to run things inside of a task for us.
        let res = f.wait();
        println!("res:{:?}", res);
    }

    #[test]
    fn should_drop_resources_on_cancel() {
        struct MyDroppable(atomic::AtomicBool);
    }
}
