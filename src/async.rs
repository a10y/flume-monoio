use futures_util::stream::StreamExt;

#[derive(Clone, Copy, Debug)]
enum ComputeRequest {
    Add(usize, usize),
    Sub(usize, usize),
    Mul(usize, usize),
    Div(usize, usize),
}

impl ComputeRequest {
    fn check_answer(self, result: ComputeResult) {
        let ComputeResult(answer) = result;

        match self {
            Self::Add(a, b) => assert_eq!(answer, a.wrapping_add(b)),
            Self::Sub(a, b) => assert_eq!(answer, a.wrapping_sub(b)),
            Self::Mul(a, b) => assert_eq!(answer, a.wrapping_mul(b)),
            Self::Div(a, b) => assert_eq!(answer, a.wrapping_div(b)),
        }
    }
}

struct ComputeResult(usize);

fn main() {
    // All threads share a single channel on which they contend to receive new work items.
    let (tx, rx) = flume::bounded(100_000_000);

    // launch all of our worker threads
    let threads: Vec<_> = (0..=2)
        .map(|tid| {
            let rx = rx.clone();

            std::thread::spawn(move || {
                println!("[INIT] worker {tid}...");
                // bind to CPU of TID
                monoio::utils::bind_to_cpu_set([tid])
                    .unwrap_or_else(|e| panic!("failed binding {tid} to CPU set: {e}"));

                println!("[BIND] worker {tid} now bound to core {tid}");

                let mut rt = monoio::RuntimeBuilder::<monoio::IoUringDriver>::new()
                    .enable_all()
                    .build()
                    .expect("building runtime");

                rt.block_on(async move {
                    //let handle = monoio::spawn(async move {
                    println!("[WORKER] {tid} entering main event loop");
                    // Why is only one of these running at a time?
                    // Or sometimes none of them?
                    let mut solved = 0;
                    //let mut msg_stream = rx.into_stream();
                    while let Ok(next_msg) = rx.recv_async().await {
                        let answer = match next_msg {
                            ComputeRequest::Add(a, b) => ComputeResult(a.wrapping_add(b)),
                            ComputeRequest::Sub(a, b) => ComputeResult(a.wrapping_sub(b)),
                            ComputeRequest::Mul(a, b) => ComputeResult(a.wrapping_mul(b)),
                            ComputeRequest::Div(a, b) => ComputeResult(a.wrapping_div(b)),
                        };

                        // check our work
                        next_msg.check_answer(answer);
                        //complete_tx.send_async(()).await.expect("sending completion msg");
                        solved += 1;
                        if solved % 1_000_000 == 0 {
                            println!("[PROGRESS] {tid} has solved {solved}");
                        }
                    }
                    println!("[COMPLETE] {tid} solved {solved} problems in total");
                    //});

                    //handle.await
                });
            })
        })
        .collect();

    // main process runs on CPU 3
    monoio::utils::bind_to_cpu_set([3]).expect("binding driver thread");

    // Submit 100MM jobs into the workers.
    println!("[DRIVER] sending jobs...");
    (0..100_000_000_usize).for_each(|num| {
        let req = match num % 4 {
            0 => ComputeRequest::Add(num, num / 2),
            1 => ComputeRequest::Sub(num, num / 2),
            2 => ComputeRequest::Mul(num, num / 2),
            3 => ComputeRequest::Div(num, num / 2),
            _ => unreachable!("math is broken, all best are off"),
        };

        // Send the request over the channel. Only one will complete it.
        tx.send(req).expect("sending math problem to worker");
    });

    println!("[DRIVER] dropping sender so children exit...");
    //drop(rx);
    drop(tx);

    println!("[DRIVER] awaiting worker threads to exit...");

    // wait for all of the handles
    for thread in threads {
        thread.join().unwrap();
    }
    println!("[DRIVER] all worker threads complete");
}
