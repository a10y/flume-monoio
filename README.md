## flume-monoio

MonoIO is an async runtime with builtin support for io-uring, a new(ish) Linux kernel facility that allows submitting
batches of syscalls, ideal for network servers or programs that do a lot of I/O.

Aside from io_uring support, MonoIO also takes on a **thread-per-core** approach to scheduling. By contrast, tokio
uses a worker threadpool on which it schedules tasks, with work-stealing between the different threads.

As an example of the different, the following Tokio code:

```rust
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[tokio::main(worker_threads = 3)]
pub async fn main() {
    let mut file = File::create("/tmp/hello-tokio")
        .await
        .unwrap();
    let bytes_written = file.write("world".as_bytes()).await.unwrap();
    println!("wrote to file!");
    assert_eq!(bytes_written, "world".len());
}
```

Will output

```
"wrote to file!"
```


But, make a tiny change to use `monoio`...


```rust
use monoio::fs::File;
use monoio::io::AsyncWriteRent;

#[monoio::main(threads = 3)]
pub async fn main() {
    let mut file = File::create("/tmp/hello-monoio")
        .await
        .expect("opening /tmp/hello-monoio");
    let (bytes_written, _) = file.write("world".to_string().into_bytes()).await;
    println!("wrote to file!");
    assert_eq!(bytes_written.expect("file write"), "world".len());
}
```

And we get the unfortunate output

```
"wrote to file!"
"wrote to file!"
"wrote to file!"
```

This is because under the hood, the `monoio::main(threads = 3)` macro is actually doing the following

1. Spawns 2 background threads
2. Creates a new monoio `Runtime` and sets each to `block_on` the future containing the function body
3. In the foreground, constructs a new `Runtime` and has it `block_on` the function body future as well

So the logic is actually replicated per-thread, and ideally you size the number of workers to be equal to the number of
cores on your server.

This is a bit gross for file handling, but it's great for a network server. If the body was not writing to a file, but
instead accepting connections for a `TcpStream`, monoio will handle replicating this across worker threads for you.

It even makes sure to set the SO_REUSEPORT option on the socket so that the kernel can handle distributing requests across
the threads.

> But what if we don't want that?

The fixed file I/O example is just one of many instances where the `monoio::main` macro behavior isn't desirable. Sometimes
you may have work to do and there isn't any easy way like the SO_REUSEPORT trick to get the kernel to distribute work for you.

For that, you can use a multi-producer, multi-consumer (MPMC) async-aware channel to distribute the work for you. Flume implements
this, so you can create a new unbounded channel, then when you spawn worker threads for your monoio `Runtime`s, each can keep
a handle to the `Receiver` end of the channel. Flume takes care of ensuring that exactly one `Receiver` will get any message. So
scheduling work becomes just producing new task definitions onto a `flume::Sender`.

You can checkout how this works in the `async.rs` file in this repo.

## Gotchas

There is one weird thing, which I have not totally figured out. If we do not include the `"sync"` feature in monoio


```toml
[dependencies]
flume = "0.11.0"
futures-util = "0.3.31"
monoio = { version = "0.2.4", features = ["default", "sync"] }
                                                     ^^^^^^
                                                    this guy
```

We end up just blocking forever. It seems like somehow, whatever flume is doing to implement its `RecvFut`
does not play nicely by default with monoio's waking behavior. 🤷‍♂️
