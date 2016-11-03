# context_bind
Binds I want out of context-rs

[Documents (broke)](https://valarauca.github.io/context_bind/context_bind/index.html)

These are simple bindings to context lib. The goal is to make a more object orriented runtime. With a free return function,
that can be more easily embedded within external libraries (the goal is to make it easier to embed async-mio functionality).

```rust

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

```

To integrate use (this is broken atm)

```
[dependencies]
context_bindings = "0.0.1"
```

