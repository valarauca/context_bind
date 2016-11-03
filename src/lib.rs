//Copyright 2016 William Cody Laeder
//
//Licensed under the Apache License, Version 2.0 (the "License");
//you may not use this file except in compliance with the License.
//You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
//Unless required by applicable law or agreed to in writing, software
//distributed under the License is distributed on an "AS IS" BASIS,
//WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//See the License for the specific language governing permissions and
//limitations under the License.


//!This crate exists to be a shim between Context and a higher level library.
//!The interface that it will present are generally not as safe as context.
//!This is an intentional design decision to make the implementation of a
//!co-routine library easier on myself. 
//!
//!The only stack this may returned is a ProtectedFixedSizedStack. It provides
//!wrapping to make creating/and interacing easier. This also means stack
//!overflows are checked.
//!
//!      context::{StackSize,Routine,swap};
//!      let lambda = Box::new(||{
//!         for i in 0usize.. {
//!             swap(i*2);
//!         }
//!      });
//!      let lambda2 = Box::new(||{
//!         for i in 0usize.. {
//!             swap(i*4);
//!         }
//!      });
//!      let mut dut0 = Routine::new(lambda,1,StackSize::KiB8).unwrap();
//!      let mut dut1 = Routine::new(lambda2,2,StackSize::KiB8).unwrap();
//!      for x in 0..10 {
//!         let a = dut0.exec(0);
//!         let b = dut1.exec(0);
//!         assert_eq!(a,x*2);
//!         assert_eq!(b,x*4);
//!      }
//!
//!The presented interface is very small. In simplest terms the value passed
//!to `exec` will be returned by `swap` and the value passed to `swap` will be
//!returned by `exec`. 
//!
//!`swap` will panic if it called outside of a coroutine.
//!
extern crate context;
use context::stack::{ProtectedFixedSizeStack, StackError};
use context::context::{Context,Transfer};
#[allow(unused_imports)]
use std::thread;
use std::cell::RefCell;
use std::mem;
use std::ptr::read as read_ptr;
use std::ptr::write as write_ptr;
const KILOBIT: isize = 1024;
const EXIT: usize = ::std::usize::MAX;
const START: usize = ::std::usize::MAX-1;

///Wraps a type from the context library
pub type SafeStack = ProtectedFixedSizeStack;
///A stack allocatd Lambda
pub type FN = Box<Fn()>;
///The ID of a Routine
pub type RoutineID = usize;

///Define the size of a stack
///
///The default stack size on most systems is 8MiB. Generally speaking you will
///lose the _light weight_ feel of M:N threading as you approach 8MiB. Smaller
///threads are _better_ but too small and you risk a lot of crashing programs
///to stack overflows.
#[derive(Copy,Clone,Debug)]
pub enum StackSize {
    KiB4 = KILOBIT*4,
    KiB8 = KILOBIT*8,
    KiB16 = KILOBIT*16,
    KiB32 = KILOBIT*32,
    KiB64 = KILOBIT*64,
    KiB128 = KILOBIT*128,
    KiB256 = KILOBIT*256,
    KiB512 = KILOBIT*512,
    MiB = KILOBIT*KILOBIT,
    MiB2 = KILOBIT*KILOBIT*2,
    MiB4 = KILOBIT*KILOBIT*4,
    MiB8 = KILOBIT*KILOBIT*8,
}

///The status of a co-routine. Not getting super in depth here.
#[derive(Copy,Clone,Debug)]
pub enum Status {
    Ready,
    Blocked
}

//Return a new stack.
//
//This function allocates a new protected fixed sized stack. If it returns
//a ERR that means your process has a smaller stack then you've attempted
//to allocate. 
//
//Err will contain the maximum stack sized for your system.
#[inline(always)]
fn new_stack(s: StackSize) -> Result<SafeStack,usize> {
    match ProtectedFixedSizeStack::new( s as usize ) {
        Ok(x) => Ok(x),
        //this is impossible if you reach the source code
        Err(StackError::IoError(e)) => panic!("\n\nSystem Error\n{:?}",e),
        Err(StackError::ExceedsMaximumSize(x)) => Err(x)
    }
}

//Used in the return to parent method
thread_local! {
    pub static THREADHANDLE: RefCell<(Option<Transfer>,Option<FN>)>
        = RefCell::new( (None,None) );
}

//Returns a thread _handle_ It is called inside the function that sets up
//the call frame for a lambda.The main job is to give the co-routines a mutable
//reference to their threadhandle
#[inline(always)]
fn thread_handle<'a>() -> &'a mut (Option<Transfer>,Option<FN>) {
    THREADHANDLE.with( |cell| {
        unsafe{ match cell.as_ptr().as_mut() {
            Option::None => panic!("Are you in a co-routine?"),
            Option::Some(x) => x
        }}
    })
}


//This is the fuction that is always called into when building a new stack.
//This is totally and completely stupidly unsafe. One must always ensure
//the THREADHANDLE in thread local is set before this function is called it
//holds on its stack a mutable reference to that value.
extern "C" fn build_stack(t: Transfer) -> ! {
    let mut local_handle = thread_handle();
    local_handle.0 = Some(t);
    match local_handle.1 {
        Option::None => panic!("Are you in a co-routine?"),
        Option::Some(ref x) => {
            (x)();
        }
    };
    swap(EXIT);
    panic!("Something horrible happened!");
}

///Leave Co-Routine.
///
///This function serves as both entry and exit point for a co-routine.
///So the function it performs is two fold
///
///This function does a boat load of unsafe things internally to manage the
///local thread state and swap contexts.
#[inline(always)]
pub fn swap(data: usize) -> usize {
    type ORG = (Option<Transfer>,Option<FN>);
    type ITEM = *mut ORG;
    unsafe{
        let ptr = thread_handle();
        let val: usize = mem::transmute(ptr);
        let ptr0: *const ORG = mem::transmute(val.clone());
        let ptr1: *mut ORG = mem::transmute(val.clone());
        if let (Some(t),_) = read_ptr(ptr0) {
            let t_new = t.context.resume(data);
            let rv: usize = t_new.data;
            let t_new = Some(t_new);
            let x: ORG = (t_new,None);
            write_ptr(ptr1, x);
            return rv;
        } else {
            panic!("Transfer does not exist!!");
        }
    }
}


///Encapsulate the state of a co-routine
///
///This holds the entire state of a co-routine. Some fields are public to
///allow for eaiser inspection.
#[allow(dead_code)]
pub struct Routine {
    pub data: usize,
    pub init: bool,
    pub id: RoutineID,
    pub state: Status,
    lambda: Option<FN>,
    stack: SafeStack,
    context: Option<Transfer>
}
impl Routine {

    ///Build a new routine. 
    ///
    ///If you call this in a routine horrible things will
    ///happen. Your new routine will panic on entry, and the old routine will 
    ///panic on exit. 
    ///
    ///This function will return an error if the stack created is too large.
    ///The maximum stack size your process may allocate will be listed in
    ///the `Err` field if that occurs.
    pub fn new(b: FN, id: RoutineID, stack: StackSize)
    -> Result<Routine,usize>
    {
        let stack = match new_stack(stack) {
            Err(e)=> return Err(e),
            Ok(x) => x,
        };
        let t = Transfer::new(Context::new(&stack,build_stack),START);
        Ok(Routine {
            data: START,
            init: false,
            id: id,
            state: Status::Ready,
            lambda: Some(b),
            stack: stack,
            context: Some(t)
        })
    }
    
    #[inline(always)]
    fn init_run(&mut self, data: usize) -> usize {
        let mut ptr = thread_handle();
        mem::swap(&mut self.lambda, &mut ptr.1);
        self.init = true;
        self.run_item(data)
    }

    #[inline(always)]
    fn run_item(&mut self, data: usize) -> usize {
        if let Some(t) = mem::replace( &mut self.context, None){
            let context = t.context.resume(data);
            let data = context.data;
            self.context = Some(context);
            return data;
        } else {
            panic!("Routine has no child to resume too");
        }
    }

    ///Run the Routine.
    ///
    ///This behaves identical to swap, except it is attached to an object.
    pub fn exec(&mut self, data: usize) -> usize {
        if self.init {
            self.run_item(data)
        } else {
            self.init_run(START)
        }
    }
}
        
#[test]
fn test_single() {
    let lambda: FN = Box::new(||{
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

#[test]
fn test_multiple() {
    let lambda = Box::new(||{
        for i in 0usize.. {
            swap(i*2);
        }
    });
    let lambda2 = Box::new(||{
        for i in 0usize.. {
            swap(i*4);
        }
    });
    let mut dut0 = Routine::new(lambda,1,StackSize::KiB8).unwrap();
    let mut dut1 = Routine::new(lambda2,2,StackSize::KiB8).unwrap();
    for x in 0..10 {
        let a = dut0.exec(0);
        let b = dut1.exec(0);
        assert_eq!(a,x*2);
        assert_eq!(b,x*4);
    }
}
