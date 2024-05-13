#[cfg(test)]
mod tokio_test {
    async fn my_async_fun() -> String {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        "123".into()
    }
    #[test]
    fn test_tokio_select() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.spawn(async {
            tokio::select! {
            sleep_1s= tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                let mut count=0;
                // while count < 10 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    println!(" sleep 1s");
                    count+=1;
                    // }
                }
                test_result =my_async_fun()=>{
                    println!(" test_result:{}",test_result);
                }
            }
        });
        rt.block_on(async {
            tokio::join!(result).0.unwrap();
        });
        println!("spawn done");
    }
}
