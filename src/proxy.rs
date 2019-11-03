use crate::error::Error;
use crate::util::Timer;
use std::cmp::Ordering;
use std::path::PathBuf;
use std::fs::File;
use std::collections::BinaryHeap;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use rand::{rngs::OsRng, Rng};
use reqwest::{Client, Proxy};
use std::sync::Mutex;

pub struct ProxyManager {
    proxy_filename : PathBuf,
    proxies : Mutex<BinaryHeap<ProxyClient>>,
    proxy_urls : Mutex<HashSet<String>>,
    stat_timer : Mutex<Timer>,
}

impl ProxyManager {
    pub fn new(proxy_filename : PathBuf) -> Result<Self, Error> {
        let pm = ProxyManager {
            proxy_filename,
            proxies : Mutex::new(BinaryHeap::new()),
            proxy_urls : Mutex::new(HashSet::new()),
            stat_timer : Mutex::new(Timer::new(Duration::from_secs(15))),
        };

        pm.read_new_proxies()?;

        Ok(pm)
    }

    pub fn get_clients<'a>(&'a self, n : usize) -> Vec<ProxyReturnWrapper<'a>> {

        let mut proxies = self.proxies.lock().unwrap();
        let mut stat_timer = self.stat_timer.lock().unwrap();

        if stat_timer.check_and_reset() {
            println!("\nNumber of Proxies: {}, Min Latency: {}ms\n",
                proxies.len(), proxies.peek()
                .map(|p| { p.latency.as_millis() })
                .unwrap_or(0));
            stat_timer.reset();
        }

        let n = std::cmp::min(proxies.len(), n);

        let thresh = 2.0 / (n as f32);

        let mut clients = Vec::<ProxyClient>::with_capacity(n);
        let mut rej_clients = Vec::<ProxyClient>::with_capacity(proxies.len());

        while clients.len() < n && proxies.len() > 0 {
            let n_remaining = n - clients.len();
            let r : f32 = OsRng.gen();

            if proxies.len() <= n_remaining || r < thresh {
                clients.push(proxies.pop().unwrap());
            } else {
                rej_clients.push(proxies.pop().unwrap());
            }
        }

        for c in rej_clients {
            proxies.push(c);
        }

        clients.into_iter().map(|c| ProxyReturnWrapper {
            client: Some(c),
            manager: &self
        }).collect()
    }

    fn return_client(&self, client: ProxyClient) {
        if ! client.bad() {
            self.proxies.lock().unwrap().push(client);
        } else {
            println!("\nDropping Proxy {}\n", client.url);
        }
    }

    pub fn read_new_proxies(&self) -> Result<(), Error> {
        use std::io::{BufReader, BufRead};

        if self.proxy_filename.as_os_str().is_empty() {
            return Ok(());
        }

        let mut proxies = self.proxies.lock().unwrap();
        let mut proxy_urls = self.proxy_urls.lock().unwrap();

        let proxy_f = File::open(&self.proxy_filename)?;
        let reader = BufReader::new(proxy_f);

        for line in reader.lines() {
            let line = line?;

            if let Some(_) = proxy_urls.get(&line) {
                //println!("Duplicate Proxy {}", l);
                continue;
            }

            match Proxy::http(&line) {
                Ok(proxy) =>  {
                    match Client::builder()
                        .timeout(Duration::from_secs(8))
                        .gzip(false) 
                        .proxy(proxy)
                        .build() {
                        Ok(proxyc) => {
                            println!("New Proxy {}", line);

                            proxies.push(ProxyClient::new(proxyc, line.clone()));
                            proxy_urls.insert(line);
                        },
                        Err(e) => {
                            println!("Failed to build proxy: {:?}", e);
                        }
                    }
                },
                Err(e) => {
                    println!("Bad Proxy {}: {:?}", line, e);
                }
            }
        }

        Ok(())
    }
}

pub struct ProxyReturnWrapper<'a> {
    client : Option<ProxyClient>,
    manager : &'a ProxyManager,
}

impl<'a> ProxyReturnWrapper<'a> {
    pub fn proxy_client(&mut self) -> &mut ProxyClient {
        self.client.as_mut().unwrap()
    }
}

impl<'a> Drop for ProxyReturnWrapper<'a> {
    fn drop(&mut self) {
        self.manager.return_client(self.client.take().unwrap());
    }
}


pub struct ProxyClient {
    client : Client,
    url : String,
    latency : Duration,
    last_success : Instant,
    fail_count : u32,
}

pub struct ProxyClientGuard<'a> {
    proxyclient : &'a mut ProxyClient,
    start : Instant,
    success : bool,
}

impl ProxyClient {
    fn new(client : Client, url : String) -> Self {
        Self {
            client,
            url,
            latency : Duration::from_secs(1),
            last_success : Instant::now(),
            fail_count : 0,
        }
    }
}

impl ProxyClient {
    pub fn access<'a>(&'a mut self) -> ProxyClientGuard<'a> {
        ProxyClientGuard {
            proxyclient : self,
            start : Instant::now(),
            success : false,
        }
    }

    pub fn bad(&self) -> bool {
        self.fail_count > 100 &&
            self.last_success.elapsed().as_secs() > 600
    }
}

impl<'a> ProxyClientGuard<'a> {
    pub fn client(&'a self) -> &'a Client {
        &self.proxyclient.client
    }

    pub fn success(mut self) {
        self.success = true;
    }
}

impl Drop for ProxyClientGuard<'_> {
    fn drop(&mut self) {
        let latency = self.start.elapsed();

        self.proxyclient.latency = (self.proxyclient.latency + latency) / 2;

        if self.success {
            self.proxyclient.last_success = Instant::now();
            self.proxyclient.fail_count = 0;
        } else {
            self.proxyclient.latency += Duration::from_secs(1);
            self.proxyclient.fail_count += 1;
        }
    }
}

impl Ord for ProxyClient {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.partial_cmp(&other) {
            Some(o) => o,
            None => Ord::cmp(&(self as *const _), &(other as *const _)),
        }
    }
}

impl PartialOrd for ProxyClient {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.latency.cmp(&other.latency) {
            // For Min Heap
            Ordering::Greater => Some(Ordering::Less),
            Ordering::Less => Some(Ordering::Greater),
            Ordering::Equal => None,
        }
    }
}

impl Eq for ProxyClient {}

impl PartialEq for ProxyClient {
    fn eq(&self, other: &Self) -> bool {
        // Address compare
        &self.client as *const _ == &other.client as *const _
    }
}
