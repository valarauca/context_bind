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

//another attempt to trigger a status build

//!This crate exists to be a shim between Context and a higher level library.
//!The interface that it will present are generally not as safe as context.
//!This is an intentional design decision to make the implementation of a
//!co-routine library easier on myself. 
//!
//!Every new routine generated will allocate generally twice. Once for the
//!lambda (the developer will do this before calling `Routine:new`) and
//!once internal to `Routine::new` to build the stack.
//!
//!Stack overflows are checked.
//!
//!To integrate this crate into your project simply use
//!```
//![dependencies]
//!context_bindings = "0.0.2"
//!```
//!
//!Below is a simple example
//!
//!    use context_bind::{StackSize,Routine,swap};
//!
//!    let mut dut0 = Routine::new(StackSize::KiB8,move ||{
//!        for i in 0usize.. {
//!            swap(i*2);
//!        }
//!    }).unwrap();
//!    let mut dut1 = Routine::new(StackSize::KiB8,move ||{
//!        for i in 0usize.. {
//!            swap(i*4);
//!        }
//!    }).unwrap();
//!    for x in 0..10 {
//!        let a = dut0.exec(0);
//!        let b = dut1.exec(0);
//!        assert_eq!(a,x*2);
//!        assert_eq!(b,x*4);
//!    }
//!
//!The presented interface is very small. In simplest terms the value passed
//!to `exec` will be injected, and returned by `swap`. The opposite is also
//!true. The value give to `swap`, will be injected and returned by `exec`.
//!
//!The `exec` function will always resume _within_ the swap call, that yielded
//!the co-routine context.
//!
//!There is more thread safety worth discussing. A routine maybe sent another
//!thread once contructed (this is safe). A routine can be sent between threads
//!while it is not running. But if you move a routine while it is running
//!(you a dark unsafe wizard), bad things _may_ happen.
//!
//!`swap` will panic if it called outside of a coroutine.
//!
//!
//!What is the difference between 0.0.1 and 0.0.2? I cleaned up the docs and
//!public interfaces.
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

type SafeStack = ProtectedFixedSizeStack;
type FN = Box<Fn()>;

///Define the size of a stack
///
///The default stack size on most systems is 8MiB. Generally speaking you will
///lose the _light weight_ feel of M:N threading as you approach 8MiB. Smaller
///threads are _better_ but too small and you risk crashing programs due
///to stack overflows.
///
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

//
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
    static THREADHANDLE: RefCell<(Option<Transfer>,Option<FN>)>
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
    if let Some(x) = mem::replace(&mut local_handle.1,None) {
        (x)();
    } else {
        panic!("Could not locate function to build routine with.");
    }
    swap(EXIT);
    panic!("Something horrible happened!");
}

///Leave Co-Routine.
///
///This function serves as both entry and exit point for a co-routine.
///
///    use context_bind::{Routine,StackSize,swap};
///    let mut dut = match Routine::new(StackSize::KiB8, move|| {
///                             //everything here
///                             //is executed on first
///                             //call
///     for i in 0usize.. {
///         swap(i);            //yield to parent
///                             //swap call ends
///                             //when parent calls
///                             //exec
///     }
///    }){
///        Ok(x) => x,
///        Err(e) => panic!("\n\nCould not allocate stack.\n{:?}\n",e)
///    };
///    for x in 0..10 {
///        let i = dut.exec(0); //run routine
///                             //this function returns
///                             //when routine calls `swap`
///        assert_eq!(x,i);
///    }
///
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
    init: bool,
    lambda: Option<FN>,
    stack: SafeStack,
    context: Option<Transfer>
}
impl Routine {

    ///Build a new routine. 
    ///
    ///This function will return an error if the stack created is too large.
    ///The maximum stack size your process may allocate will be listed in
    ///the `Err` field if that occurs.
    ///
    pub fn new<F>(stack: StackSize, b: F)
    -> Result<Routine,usize>
    where
        F: Fn()+'static
    {
        let stack = match new_stack(stack) {
            Err(e)=> return Err(e),
            Ok(x) => x,
        };
        let t = Transfer::new(Context::new(&stack,build_stack),START);
        Ok(Routine {
            data: START,
            init: false,
            lambda: Some(Box::new(b)),
            stack: stack,
            context: Some(t)
        })
    }
   
    ///If you don't want to allocate internally at all
    pub fn no_func_alloc<F>(stack: StackSize, b: Box<F>)
    -> Result<Routine,usize>
    where
        F: Fn()+'static
    {
        let stack = match new_stack(stack) {
            Err(e) => return Err(e),
            Ok(x) => x,
        };
        let t = Transfer::new(Context::new(&stack,build_stack),START);
        Ok(Routine {
            data: START,
            init: false,
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
    let mut dut = match Routine::new(StackSize::KiB8, move ||{
        for i in 0usize.. {
            swap(i);
        }
    }){
        Ok(x) => x,
        Err(e) => panic!("\n\nCould not allocate stack.\n{:?}\n",e)
    };
    for x in 0..10 {
        let i = dut.exec(0);
        assert_eq!(x,i);
    }
}

#[test]
fn test_multiple() {
    let mut dut0 = Routine::new(StackSize::KiB8, move ||{
        for i in 0usize.. {
            swap(i*2);
        }
    }).unwrap();
    let mut dut1 = Routine::new(StackSize::KiB8, move||{
        for i in 0usize.. {
            swap(i*4);
        }
    }).unwrap();
    for x in 0..10 {
        let a = dut0.exec(0);
        let b = dut1.exec(0);
        assert_eq!(a,x*2);
        assert_eq!(b,x*4);
    }
}
