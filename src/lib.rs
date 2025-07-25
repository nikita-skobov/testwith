use std::pin::Pin;

use tokio::{sync::mpsc::{UnboundedReceiver, UnboundedSender}, task::{JoinError, JoinHandle}};

pub trait FnWithSomeAmountOfArgs {
    type Args;
    fn call_with_args(&self, f: Self::Args);
}

pub trait FnWithSomeAmountOfArgsAsync {
    type Args;
    type Fut: Future<Output = ()> + Send;
    fn call_with_args(&self, f: Self::Args) -> Self::Fut;
}

pub trait Undo: Send {
    fn undo(&mut self);
}

macro_rules! impl_undo_tuple {
    ($( $($gen:ident),* );*) => {
        $(
            #[allow(non_snake_case, unused)]
            impl<$($gen: Undo),*> Undo for ($($gen),*) {
                fn undo(&mut self) {
                    let ($($gen),*) = self;
                    println!("deleting tuple");
                    $(
                        $gen.undo();
                    )*
                }
            }
        )*
    };
}

macro_rules! impl_fn_args {
    ($( $($gen:ident),* );*) => {
        $(
            #[allow(non_snake_case, unused)]
            impl<$($gen),*> FnWithSomeAmountOfArgs for Box<dyn Fn($($gen),*)> {
                type Args = ($($gen, )*);
                fn call_with_args(&self, f: Self::Args) {
                    let ($($gen, )*) = f;
                    self($($gen, )*);
                }
            }
            #[allow(non_snake_case, unused)]
            impl<$($gen: 'static,)* F: Fn($($gen,)*) + 'static> From<F> for Box<dyn FnWithSomeAmountOfArgs<Args = ($($gen,)*)>> {
                fn from(value: F) -> Self {
                    let b: Box<dyn Fn($($gen,)*)> = Box::new(value);
                    let b: Box<dyn FnWithSomeAmountOfArgs<Args = ($($gen,)*)>> = Box::new(b);
                    b
                }
            }

            #[allow(non_snake_case, unused)]
            impl<$($gen),* , Fut> FnWithSomeAmountOfArgsAsync for Pin<Box<dyn Fn($($gen),*) -> Fut + Send + 'static>>
                where Fut: Future<Output = ()> + Send
            {
                type Args = ($($gen, )*);
                type Fut = Fut;
                fn call_with_args(&self, f: Self::Args) -> Self::Fut {
                    let ($($gen, )*) = f;
                    self($($gen, )*)
                }
            }

            #[allow(non_snake_case, unused)]
            impl<$($gen: 'static,)* Futur, F: Fn($($gen,)*) -> Futur + Send + 'static> From<F> for Pin<Box<dyn FnWithSomeAmountOfArgsAsync<Args = ($($gen,)*), Fut = Futur> + Send + 'static>>
                where Futur: Future<Output = ()> + Send + 'static
            {
                fn from(value: F) -> Self {
                    let b: Pin<Box<dyn Fn($($gen,)*) -> Futur + Send + 'static>> = Box::pin(value);
                    let b: Pin<Box<dyn FnWithSomeAmountOfArgsAsync<Args = ($($gen,)*), Fut = Futur> + Send + 'static>> = Box::pin(b);
                    b
                }
            }
        )*
    };
}

impl_fn_args!(
    T1, T2;
    T1, T2, T3;
    T1, T2, T3, T4;
    T1, T2, T3, T4, T5;
    T1, T2, T3, T4, T5, T6
    // if you got this far its your fault. dont have functions with more than 6 args
);

impl_undo_tuple!(
    T1, T2;
    T1, T2, T3;
    T1, T2, T3, T4;
    T1, T2, T3, T4, T5;
    T1, T2, T3, T4, T5, T6
    // if you got this far its your fault. dont have functions with more than 6 args
);


#[derive(Clone)]
pub struct TestContext {
    pub task_tx: UnboundedSender<(String, JoinHandle<()>)>,
    pub undo_tx: UnboundedSender<Box<dyn Undo>>,
}

#[derive(Clone)]
pub struct TestWith<T> {
    pub ctx: TestContext,
    pub t: T,
}

impl<T: Undo + Clone + 'static> TestWith<T> {
    pub fn test_without<F: Fn() + Send + 'static>(&self, f: F) -> &Self {
        let fn_name = std::any::type_name::<F>();
        let task = tokio::task::spawn(async move {
            f();
        });
        let _ = self.ctx.task_tx.send((fn_name.to_string(), task));        
        self
    }
    pub fn test_without_async<Fut, F: Fn() -> Fut + Send + 'static>(&self, f: F) -> &Self
        where Fut: Future<Output = ()> + Send + 'static
    {
        let fn_name = std::any::type_name::<F>();
        let task = tokio::task::spawn(async move {
            f().await;
        });
        let _ = self.ctx.task_tx.send((fn_name.to_string(), task));        
        self
    }
    pub fn test<'a, F: Into<Box<dyn FnWithSomeAmountOfArgs<Args = T>>> + Send + 'static>(&self, f: F) -> &Self
    {
        let fn_name = std::any::type_name::<F>();
        let t_clone = self.t.clone();
        let task = tokio::task::spawn(async move {
            let f_box: Box<dyn FnWithSomeAmountOfArgs<Args = T>> = f.into();
            f_box.call_with_args(t_clone);
        });
        let _ = self.ctx.task_tx.send((fn_name.to_string(), task));
        self
    }
    pub fn test_async<Futur, F: Into<Pin<Box<dyn FnWithSomeAmountOfArgsAsync<Args = T, Fut = Futur> + Send + 'static>>> + Send + 'static>(&self, f: F) -> &Self
        where Futur: Future<Output = ()> + Send + 'static
    {
        let fn_name = std::any::type_name::<F>();
        let t_clone = self.t.clone();
        let task = tokio::task::spawn(async move {
            let f_box: Pin<Box<dyn FnWithSomeAmountOfArgsAsync<Args = T, Fut = Futur> + Send + 'static>> = f.into();
            f_box.call_with_args(t_clone).await;
        });
        let _ = self.ctx.task_tx.send((fn_name.to_string(), task));
        self
    }
    pub fn test_one<'a, F: Fn(T) + Send + 'static>(&self, f: F) -> &Self
    {
        let fn_name = std::any::type_name::<F>();
        let t_clone = self.t.clone();
        let task = tokio::task::spawn(async move {
            f(t_clone);
        });
        let _ = self.ctx.task_tx.send((fn_name.to_string(), task));
        self
    }
    pub fn test_one_async<'a, Futur, F: Fn(T) -> Futur + Send + 'static>(&self, f: F) -> &Self
        where Futur: Future<Output = ()> + Send + 'static
    {
        let fn_name = std::any::type_name::<F>();
        let t_clone = self.t.clone();
        let task = tokio::task::spawn(async move {
            f(t_clone).await;
        });
        let _ = self.ctx.task_tx.send((fn_name.to_string(), task));
        self
    }
    pub fn transform<T2: Undo + Clone + 'static, F: Fn(T) -> T2>(&self, f: F) -> TestWith<T2> {
        let TestWith { ctx, t } = self.clone();
        let t2 = f(t);
        let _ = ctx.undo_tx.send(Box::new(t2.clone()));
        TestWith { ctx, t: t2 }
    }
    pub async fn transform_async<Fut, T2: Undo + Clone + 'static, F: Fn(T) -> Fut>(self, f: F) -> TestWith<T2>
        where Fut: Future<Output = T2>,
    {
        let TestWith { ctx, t } = self;
        let t2 = f(t).await;
        let _ = ctx.undo_tx.send(Box::new(t2.clone()));
        TestWith { ctx, t: t2 }
    }
}

impl TestContext {
    pub fn create<T: Undo + Clone + 'static>(&self, t: T) -> TestWith<T> {
        let _ = self.undo_tx.send(Box::new(t.clone()));
        TestWith { ctx: self.clone(), t }
    }
}

/// holds the final receivers for the entire test run.
pub struct TestResultHolder {
    /// receives all task handles with their corresponding test name
    /// when .finish() is called, this checks for panics and reports them
    pub task_rx: UnboundedReceiver<(String, JoinHandle<()>)>,
    /// receives all Undo items as they are created.
    /// when .finish() is called, this calls .undo() for all of them to perform cleanup.
    pub undo_rx: UnboundedReceiver<Box<dyn Undo>>,
}

fn report_panic(e: JoinError) {
    let cancelled_fmt = format!("{:?}", e);
    if let Ok(e) = e.try_into_panic() {
        std::panic::resume_unwind(e);
    } else {
        panic!("task cancelled: {}", cancelled_fmt);
    }
}

impl TestResultHolder {
    /// undoes everything in the undo_rx channel,
    /// and reports panics from task_rx channel (if any)
    pub async fn finish(mut self) {
        let mut errors = vec![];
        while let Some((task_name, task_handle)) = self.task_rx.recv().await {
            if let Err(e) = task_handle.await {
                errors.push(e);
                println!("{} ... ERR", task_name);
            } else {
                println!("{} ... OK", task_name);
            }
        }
        // only process the undos after all tasks are done
        while let Some(mut msg) = self.undo_rx.recv().await {
            let msg = &mut *msg;
            msg.undo();
        }
        if errors.is_empty() { return; }
        if errors.len() == 1 {
            // simply report the panic as-is:
            report_panic(errors.remove(0));
            return; // useless return, report_panic should panic
        }
        // if we got here, there was more than 1 error, so print all first,
        // and then unwind the last one
        println!("{} tasks panicked. reporting all, unwinding the last error", errors.len());
        for err in errors.iter() {
            println!("{:?}", err);
        }
        let last_index = errors.len() - 1;
        let err = errors.remove(last_index);
        report_panic(err);
    }
}

pub fn setup_test_holder() -> (TestContext, TestResultHolder) {
    let (task_tx, task_rx) = tokio::sync::mpsc::unbounded_channel();
    let (undo_tx, undo_rx) = tokio::sync::mpsc::unbounded_channel();
    (TestContext { task_tx, undo_tx }, TestResultHolder { task_rx, undo_rx })
}

/// Example:
/// ```rs
/// #[test]
/// fn a() {
///     run_tests(async |ctx| {
///         ctx.create(DynamoTable { table_name: "a" })
///             .test_one(aaa)
///             .test_one(|a| {})
///             .transform(|d| { (d, S3Object::default())})
///             .test(|a, b| {println!("AB");})
///             .test(bbb);
///     });
/// }
/// ```
pub fn run_tests<Fut, F: FnMut(TestContext) -> Fut + Send + 'static>(mut f: F)
    where Fut: Future<Output = ()> + Send,
{
    let (ctx, holder) = setup_test_holder();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("failed to build tokio runtime");
    rt.block_on(async move {
        let tx = ctx.task_tx.clone();
        let t = tokio::task::spawn(async move {
            f(ctx).await;
        });
        let _ = tx.send(("main".to_string(), t));
        drop(tx);
        holder.finish().await;
    });
}

#[cfg(test)]
mod test {
    use std::{path::PathBuf, time::Duration};

    use super::*;

    #[derive(Clone)]
    pub struct SomeResource { pub x: u32 }
    impl Undo for SomeResource {
        fn undo(&mut self) {
            println!("deleting some resource");
        }
    }

    #[derive(Clone)]
    pub struct SomeFile { pub path: PathBuf }
    impl Undo for SomeFile {    
        fn undo(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    impl SomeFile {
        pub fn new(s: &str) -> Self {
            let p = PathBuf::from(s);
            std::fs::write(&p, "hello").expect("failed to create file");
            Self { path: p }
        }
    }

    #[test]
    #[should_panic = "eee"]
    fn can_report_panics() {
        run_tests(async |_ctx| {
            panic!("eee");
        });
    }

    #[test]
    fn can_test_with_closure() {
        run_tests(async |ctx| {
            ctx.create(SomeResource { x: 0 }).test_one(|a| {
                assert_eq!(a.x, 0);
            });
        });
    }

    #[test]
    #[should_panic]
    fn can_test_with_closure_err() {
        run_tests(async |ctx| {
            ctx.create(SomeResource { x: 0 }).test_one(|a| {
                assert_eq!(a.x, 1);
            });
        });
    }

    async fn dummy(_: SomeResource) {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    async fn dummy2(_: SomeResource, _: SomeResource) {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    #[test]
    fn can_test_async() {
        run_tests(async |ctx| {
            ctx.create(SomeResource { x: 0 })
                .test_one_async(async |_| {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                })
                .test_one_async(dummy)
                .transform(|x| (x.clone(), x))
                .test_async(dummy2)
                .test_async(async |_, _| {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                });
        });
    }

    fn test_resource(a: SomeResource) {
        assert_eq!(a.x, 0);
    }

    #[test]
    fn can_test_with_fn() {
        run_tests(async |ctx| {
            ctx.create(SomeResource { x: 0 }).test_one(test_resource);
        });
    }

    fn test_without() {}
    async fn test_without_async() {}

    #[test]
    fn can_test_without_type_t() {
        run_tests(async |ctx| {
            ctx.create(SomeResource { x: 0 })
                .test_without(|| {})
                .test_without_async(test_without_async)
                .test_without(test_without)
                .test_without_async(async || {});
        });
    }

    #[test]
    #[should_panic]
    fn can_test_with_fn_err() {
        run_tests(async |ctx| {
            ctx.create(SomeResource { x: 0 })
                .transform(|mut a| { a.x = 1; a })
                .test_one(test_resource);
        });
    }

    #[test]
    fn undo_gets_called() {
        // the file shouldnt exist yet because we havent started the test:
        assert!(!std::fs::exists("/tmp/somefile.txt").expect("it shouldnt fail"));
        run_tests(async |ctx| {
            ctx.create(SomeFile::new("/tmp/somefile.txt"))
                .test_one(|f| {
                    let f_data = std::fs::read_to_string(&f.path).expect("it shouldnt fail");
                    assert_eq!(f_data, "hello");
                });
            // the file should still exist here because we're in run_tests
            assert!(std::fs::exists("/tmp/somefile.txt").expect("it shouldnt fail"));
        });
        // now, the file shouldnt exist because we finished the test case(s)
        assert!(!std::fs::exists("/tmp/somefile.txt").expect("it shouldnt fail"));
    }

    #[test]
    #[should_panic]
    fn undo_gets_called_err() {
        // the file shouldnt exist yet because we havent started the test:
        assert!(!std::fs::exists("/tmp/somefile2.txt").expect("it shouldnt fail"));
        run_tests(async |ctx| {
            ctx.create(SomeFile::new("/tmp/somefile2.txt"))
                .test_one(|f| {
                    let f_data = std::fs::read_to_string(&f.path).expect("it shouldnt fail");
                    // if we panic here, it should still get cleaned up
                    assert_eq!(f_data, "beep");
                });
        });
        // now, the file shouldnt exist because we cleaned it up, despite panicking in the test run
        assert!(!std::fs::exists("/tmp/somefile2.txt").expect("it shouldnt fail"));
    }
}
