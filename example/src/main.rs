extern crate context_bind;
use context_bind::{StackSize,Routine,swap, Transfer};
use std::cell::RefCell;
use std::mem;
use std::thread;

thread_local!{
    static THREADHANDLE: RefCell<(Transfer,Box<Fn()>)>
        = RefCell::new( unsafe{ mem::zeroed() });
}

fn main() {

    let lambda = Box::new(||{
        for i in 0usize.. {
            print!("Yielding {:?} => ", i);
            swap(i);
        }
    });
    let mut dut = match Routine::new(lambda,1,StackSize::KiB8) {
        Ok(x) => x,
        Err(e) => panic!("\n\nCould not allocate stack.\n{:?}\n",e)
    };
    for x in 0..10 {
        print!("Resuming => ");
        let i = dut.exec(0);
        assert_eq!(x,i);
        println!("Got {:?}", i);
    }
    println!("Finished!");
}
