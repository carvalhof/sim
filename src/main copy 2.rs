extern crate rustc_serialize;
use rustc_serialize::json::Json;

mod request;
use request::Request;

mod worker_core;
use worker_core::Core;

use ::std::{
    rc::Rc,
    fs::File,
    io::Read,
    fmt::Write,
    cell::{
        RefMut,
        RefCell,
    },
    collections::{
        HashMap,
        VecDeque,
    },
};
use ::rand::{
    Rng,
    RngCore,
    rngs::SmallRng,
    SeedableRng,
};
use indicatif::{
    ProgressBar, 
    ProgressState, 
    ProgressStyle
};
use csv;
use log::info;

const INITIAL_SEED: u64 = 7;

enum Layout {
    Layout1(Core, Vec<Core>, Vec<usize>),
    Layout2(Vec<Core>),
    Layout3(Core, Vec<Core>),
    Layout4(Vec<Core>, HashMap<usize, Vec<Core>>),
}

impl std::fmt::Debug for Layout {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let l = match &self {
            Layout::Layout1(_, _, _) => 1,
            Layout::Layout2(_) => 2,
            Layout::Layout3(_, _) => 3,
            Layout::Layout4(_, _) => 4,
        };
        write!(f, "{}", l)
    }
}

pub enum CoreAction {
    Forward,
    Application,
    NetworkStack,
    NetworkStackAndApplication,
    NetworkStackAndApplicationLock,
}

pub enum CoreState {
    Idle,
    Running,
    Finished(Request),
}

struct Simulation {
    // NIC Related
    nr_indirection_table_entries: usize,
    indirection_table: HashMap<usize, usize>,

    // Server Related
    nr_total_cores: usize,
    layout: Layout,

    // Simulator Related
    run_id: usize,
    t_cur: usize,
    t_duration: usize,
    nr_packets: usize,
    received: usize,
    rtt_base: usize,
    last_workers_idx: HashMap<usize, usize>,
    dropped: VecDeque<Request>,
    dropped_per_core: Vec<usize>,
    finished: VecDeque<Request>,
    finished_per_core: Vec<usize>,
    packets: VecDeque<Request>,
    progress_bar: ProgressBar,
}

fn exponential_centered(r: f64, lambda: f64) -> usize {
    let l: f64 = -((1.0 - r).ln());
    (l / lambda) as usize
}

impl Simulation {
    pub fn new(run_id: usize, rng: Rc<RefCell<SmallRng>>) -> Simulation {
        info!("Creating the Simulation");

        let mut file: File = File::open("config.json").unwrap();
        let mut data: String = String::new();
        file.read_to_string(&mut data).unwrap();
        let json: Json = Json::from_str(&data).unwrap();

        let t_duration: usize = json.find(&"duration").unwrap().as_u64().unwrap() as usize;
        let queue_size: usize = json.find(&"queue_size").unwrap().as_u64().unwrap() as usize;
        let nr_total_cores: usize = json.find(&"nr_total_cores").unwrap().as_u64().unwrap() as usize;
        let nr_flows: u64 = json.find_path(&["packets", "nr_flows"]).unwrap().as_u64().unwrap();
        let which_layout: usize = json.find(&"layout").unwrap().as_u64().unwrap() as usize;

        let mut last_workers_idx: HashMap<usize, usize> = HashMap::<usize, usize>::new();
        let layout: Layout = match which_layout {
            1 => {
                let nr_worker_cores: usize = json.find_path(&["layout1", "nr_worker_cores"]).unwrap().as_u64().unwrap() as usize;
                if nr_worker_cores + 1 > nr_total_cores {
                    panic!("ERROR: the number of cores");
                }

                let forwarder: Core = Core::new(0, CoreAction::Forward, queue_size);

                let mut arr: Vec<Core> = Vec::<Core>::with_capacity(nr_worker_cores);
                for i in 0..nr_worker_cores {
                    let core: Core = Core::new(i + 1, CoreAction::NetworkStackAndApplicationLock, queue_size);
                    arr.push(core);
                }

                last_workers_idx.insert(forwarder.get_id(), 0);

                // Locks for each flow
                let mut locks: Vec<usize> = Vec::<usize>::with_capacity(nr_flows as usize);
                for _ in 0..nr_flows {
                    locks.push(usize::MAX);
                }
                Layout::Layout1(forwarder, arr, locks)
            },
            2 => {
                let nr_worker_cores: usize = json.find_path(&["layout2", "nr_worker_cores"]).unwrap().as_u64().unwrap() as usize;
                if nr_worker_cores > nr_total_cores {
                    panic!("ERROR: the number of cores");
                }

                let mut arr: Vec<Core> = Vec::<Core>::with_capacity(nr_worker_cores);
                for i in 0..nr_worker_cores {
                    let core: Core = Core::new(i, CoreAction::NetworkStackAndApplication, queue_size);
                    arr.push(core);
                }

                Layout::Layout2(arr)
            },
            3 => {
                let nr_application_cores: usize = json.find_path(&["layout3", "nr_application_cores"]).unwrap().as_u64().unwrap() as usize;

                if nr_application_cores + 1 > nr_total_cores {
                    panic!("ERROR: the number of cores");
                }

                let network_core: Core = Core::new(0, CoreAction::NetworkStack, queue_size);

                let mut arr: Vec<Core> = Vec::<Core>::with_capacity(nr_application_cores);
                for i in 0..nr_application_cores {
                    let core: Core = Core::new(i + 1, CoreAction::Application, queue_size);
                    arr.push(core);
                }

                last_workers_idx.insert(network_core.get_id(), 0);
                Layout::Layout3(network_core, arr)
            },
            4 => {
                let nr_network_cores: usize = json.find_path(&["layout4", "nr_network_cores"]).unwrap().as_u64().unwrap() as usize;
                let nr_application_cores: usize = json.find_path(&["layout4", "nr_application_cores"]).unwrap().as_u64().unwrap() as usize;

                if nr_network_cores + nr_application_cores > nr_total_cores {
                    panic!("ERROR: the number of cores");
                }

                if nr_application_cores < nr_network_cores {
                    panic!("ERROR: number of application core should be bigger than the number of network stack cores.");
                }

                let mut map: HashMap<usize, Vec<Core>> = HashMap::<usize, Vec<Core>>::new();
                let mut arr: Vec<Core> = Vec::<Core>::with_capacity(nr_network_cores);
                for i in 0..nr_network_cores {
                    let core_id: usize = i;
                    let core: Core = Core::new(core_id, CoreAction::NetworkStack, queue_size);
                    arr.push(core);
                    map.insert(core_id, Vec::new());
                    last_workers_idx.insert(core_id, 0);
                }

                for i in 0..nr_application_cores {
                    let core_id: usize = nr_network_cores + i;
                    let core: Core = Core::new(core_id, CoreAction::Application, queue_size);
                    let network_id: usize = core_id % nr_network_cores;
                    let arr: &mut Vec<Core> = map.get_mut(&network_id).unwrap();
                    arr.push(core);
                }

                Layout::Layout4(arr, map)
            },
            _ => panic!("ERROR: layout should be 1, 2, 3, or 4.")
        };

        let mut dropped_per_core: Vec<usize> = Vec::<usize>::with_capacity(nr_total_cores);
        let mut finished_per_core: Vec<usize> = Vec::<usize>::with_capacity(nr_total_cores);
        for _ in 0..nr_total_cores {
            dropped_per_core.push(0);
            finished_per_core.push(0);
        }

        let nr_packets: usize = json.find_path(&["packets", "nr_packets"]).unwrap().as_u64().unwrap() as usize;
        let packets: VecDeque<Request> = {
            let mut packets: Vec<Request> = Vec::<Request>::with_capacity(nr_packets);

            println!("\nConfiguring the requests...");
            let pb: ProgressBar = ProgressBar::new(nr_packets as u64);
            pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:40.green/white}] {pos:>7}/{len:7} (requests)")
            .unwrap()
            .progress_chars("#>-"));

            let mut rng_: RefMut<SmallRng>= rng.borrow_mut();
            let interarrival_distribution: &str = json.find_path(&["packets", "distribution"]).unwrap().as_string().unwrap();
            let rate: f64 = json.find_path(&["packets", "rate"]).unwrap().as_u64().unwrap() as f64;
            let forwarder_mean1: usize = json.find_path(&["forwarder", "mean1"]).unwrap().as_u64().unwrap() as usize;
            let forwarder_distribution: &str = json.find_path(&["forwarder", "distribution"]).unwrap().as_string().unwrap();
            let stack_distribution: &str = json.find_path(&["network_stack", "distribution"]).unwrap().as_string().unwrap();
            let stack_mean1: usize = json.find_path(&["network_stack", "mean1"]).unwrap().as_u64().unwrap() as usize;
            let application_distribution: &str = json.find_path(&["application", "distribution"]).unwrap().as_string().unwrap();
            let application_mean1: usize = json.find_path(&["application", "mean1"]).unwrap().as_u64().unwrap() as usize;

            let mut last_t_arrival: usize = 0;
            for i in 0..nr_packets {
                last_t_arrival = match interarrival_distribution {
                    "constant" => {
                        let inter_arrival: usize = (1_000_000_000.0 / rate) as usize;
                        last_t_arrival + inter_arrival
                    },
                    "exponential" => {
                        let inter_arrival: f64 = rate / 1_000_000_000.0;
                        last_t_arrival + exponential_centered(rng_.gen::<f64>(), inter_arrival)
                    },
                    _ => 100,
                };

                let mut last_stack_time: usize = match stack_distribution {
                    "constant" => stack_mean1,
                    "exponential" => exponential_centered(rng_.gen::<f64>(), 1.0 / stack_mean1 as f64),
                    _ => 10,
                };
                if last_stack_time == 0 {
                    last_stack_time += 1;
                }

                let mut last_application_time: usize = match application_distribution {
                    "constant" => application_mean1,
                    "exponential" => exponential_centered(rng_.gen::<f64>(), 1.0 / application_mean1 as f64),
                    "bimodal" => {
                        let application_mode: f64 = json.find_path(&["application", "mode"]).unwrap().as_f64().unwrap();
                        let application_mean2: usize = json.find_path(&["application", "mean2"]).unwrap().as_u64().unwrap() as usize;
                        let r: f64 = rng_.gen::<f64>();
                        if r < application_mode {
                            application_mean1
                        } else {
                            application_mean2
                        }
                    }
                    _ => 10,
                };
                if last_application_time == 0 {
                    last_application_time += 1;
                }

                let mut last_forwarder_time: usize = match forwarder_distribution {
                    "constant" => forwarder_mean1,
                    "exponential" => exponential_centered(rng_.gen::<f64>(), 1.0 / forwarder_mean1 as f64),
                    "bimodal" => {
                        let forwarder_mode: f64 = json.find_path(&["forwarder", "mode"]).unwrap().as_f64().unwrap();
                        let forwarder_mean2: usize = json.find_path(&["forwarder", "mean2"]).unwrap().as_u64().unwrap() as usize;
                        let r: f64 = rng_.gen::<f64>();
                        if r < forwarder_mode {
                            forwarder_mean1
                        } else {
                            forwarder_mean2
                        }
                    }
                    _ => 10,
                };
                if last_forwarder_time == 0 {
                    last_forwarder_time += 1;
                }

                let req: Request = Request::new(
                    i as usize,
                    (rng_.next_u64() % nr_flows) as usize,
                    last_t_arrival,
                    last_forwarder_time,
                    last_stack_time,
                    last_application_time,
                );

                packets.push(req);
                pb.inc(1);
            }

            // Order the packets according to arrival_time
            // packets.sort_by(|a, b| a.get_arrival_time().cmp(&b.get_arrival_time()));

            pb.finish();
            println!("Done.");

            VecDeque::from(packets)
        };
        
        let progress_bar: ProgressBar = ProgressBar::new(t_duration as u64);
        progress_bar.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:40.blue/white}] {pos:>7}/{len:7} (ticks) [{percent}]")
            .unwrap()
            .with_key("percent", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.2}%", (state.fraction() * 100.0)).unwrap())
            .progress_chars("#>-"));

        // Assuming Round-Robin for the queue_id <-> core_id relationship
        let mut indirection_table: HashMap<usize, usize> = HashMap::<usize,usize>::new();
        let nr_queues: usize = match &layout {
            Layout::Layout1(_, _, _) => 1,
            Layout::Layout2(arr) => arr.len(),
            Layout::Layout3(_, _) => 1,
            Layout::Layout4(arr, _) => arr.len(),
        };

        let nr_indirection_table_entries: usize = json.find(&"nr_indirection_table_entries").unwrap().as_u64().unwrap() as usize;
        for i in 0..nr_indirection_table_entries {
            indirection_table.insert(i, i % nr_queues);
        }

        let rtt_base: usize = json.find_path(&["rtt_base"]).unwrap().as_u64().unwrap() as usize;

        let sim: Simulation = Simulation {
            // NIC Related
            nr_indirection_table_entries,
            indirection_table,

            // Server Related
            nr_total_cores,
            layout,

            // Simulator Related
            run_id,
            t_cur: 0,
            t_duration,
            nr_packets,
            received: 0,
            rtt_base,
            last_workers_idx,
            dropped: VecDeque::<Request>::new(),
            dropped_per_core,
            finished: VecDeque::<Request>::new(),
            finished_per_core,
            packets,
            progress_bar
        };

        sim
    }

    fn select_core(&mut self, req: &Request) -> &mut Core {
        match &mut self.layout {
            Layout::Layout1(forwarder, _, _) => forwarder,
            Layout::Layout2(arr) => {
                let queue_id: usize = req.get_flow_id() % self.nr_indirection_table_entries;
                let core_id: usize = *self.indirection_table.get(&queue_id).unwrap();
                &mut arr[core_id]
            }
            Layout::Layout3(network_core, _) => network_core,
            Layout::Layout4(arr, _) => {
                let queue_id: usize = req.get_flow_id() % self.nr_indirection_table_entries;
                let core_id: usize = *self.indirection_table.get(&queue_id).unwrap();
                &mut arr[core_id]
            }
        }
    }

    fn has_remaining_requests(&self) -> bool {
        if self.dropped.len() + self.finished.len() < self.nr_packets {
            return true;
        }
        
        false
    }

    fn schedule_all_cores(&mut self) {
        match &mut self.layout {
            Layout::Layout1(forwarder, worker_cores, locks) => {
                // First, we make progress on all worker cores.
                // Starting from the last core that received a new request.
                let last_worker_idx: &usize = self.last_workers_idx.get(&forwarder.get_id()).unwrap();

                // Now, we make progress on all other workers and select the first idle worker.
                let mut idle_worker_core: Option<usize> = None;
                let mut idx: usize = *last_worker_idx;
                for core in worker_cores[*last_worker_idx..].iter_mut() {
                    match core.get_action() {
                        CoreAction::NetworkStackAndApplicationLock => {
                            if idle_worker_core.is_none() && core.is_idle() {
                                // Here, we select an idle core
                                // In this case, we make sure it is not the last one
                                idle_worker_core = Some(idx);
                            } 
                            match core.schedule(self.t_cur, Some(locks)) {
                                CoreState::Finished(req) => {
                                    log::warn!("Worker Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                    self.finished.push_back(req);
                                    self.finished_per_core[core.get_id()] += 1;
                                },
                                _ => {} // Moving on
                            }
                        },
                        _ => panic!("ERROR: should not be here.")
                    }
                    idx += 1;
                }
                idx = 0;
                for core in worker_cores[..*last_worker_idx].iter_mut() {
                    match core.get_action() {
                        CoreAction::NetworkStackAndApplicationLock => {
                            if idle_worker_core.is_none() && core.is_idle() {
                                // Here, we select an idle core
                                // In this case, we make sure it is not the last one
                                idle_worker_core = Some(idx);
                            }
                            match core.schedule(self.t_cur, Some(locks)) {
                                CoreState::Finished(req) => {
                                    log::warn!("Worker Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                    self.finished.push_back(req);
                                    self.finished_per_core[core.get_id()] += 1;
                                },
                                _ => {} // Moving on
                            }
                        },
                        _ => panic!("ERROR: should not be here.")
                    }
                    idx += 1;
                }
                // Third, we forward a packet to an idle worker (if any)
                // if let Some(worker_idx) = idle_worker_core {
                //     self.last_workers_idx.insert(forwarder.get_id(), worker_idx);
                //     let worker: &mut Core = &mut worker_cores[worker_idx];
                //     match forwarder.schedule(self.t_cur, None) {
                //         CoreState::Finished(req) => {
                //             log::warn!("Forwarded Core #{:?} finished the Request #{:?}", forwarder.get_id(), req.get_id());
                //             worker.try_enqueue(req).expect("ERROR: should not be here.");
                //         },
                //         _ => {} // Moving on
                //     }
                // }

                match forwarder.schedule(self.t_cur, None) {
                    CoreState::Finished(req) => {
                        if let Some(worker_idx) = idle_worker_core {
                            self.last_workers_idx.insert(forwarder.get_id(), worker_idx);
                            let worker: &mut Core = &mut worker_cores[worker_idx];
                            log::warn!("[{:?}]:Forwarded Core #{:?} finished the Request #{:?}", self.t_cur, forwarder.get_id(), req.get_id());
                            worker.try_enqueue(req).expect("ERROR: should not be here.");
                        }
                    },
                    _ => {} // Moving on
                }
            },
            Layout::Layout2(worker_cores) => {
                // First, we just make progress in all worker cores
                for core in worker_cores {
                    match core.get_action() {
                        CoreAction::NetworkStackAndApplication => {
                            match core.schedule(self.t_cur, None) {
                                CoreState::Finished(req) => {
                                    log::warn!("[{:?}]: Worker Core #{:?} finished the Request #{:?}", self.t_cur, core.get_id(), req.get_id());
                                    self.finished.push_back(req);
                                    self.finished_per_core[core.get_id()] += 1;
                                },
                                _ => {} // Moving on
                            }
                        },
                        _ => panic!("ERROR: should not be here.")
                    }
                }
            },
            Layout::Layout3(network_core, application_cores) => {
                // First, we make progress on all worker cores.
                // Starting from the last core that received a new request.
                let last_worker_idx: &usize = self.last_workers_idx.get(&network_core.get_id()).unwrap();

                // Now, we make progress on all other workers and select the first idle worker.
                let mut idle_worker_core: Option<usize> = None;
                let mut idx: usize = *last_worker_idx;
                for core in application_cores[*last_worker_idx..].iter_mut() {
                    match core.get_action() {
                        CoreAction::Application => {
                            if idle_worker_core.is_none() && core.is_idle() {
                                // Here, we select an idle core
                                // In this case, we make sure it is not the last one
                                idle_worker_core = Some(idx);
                            } 
                            match core.schedule(self.t_cur, None) {
                                CoreState::Finished(req) => {
                                    log::warn!("Worker Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                    self.finished.push_back(req);
                                    self.finished_per_core[core.get_id()] += 1;
                                },
                                _ => {} // Moving on
                            }
                        },
                        _ => panic!("ERROR: should not be here.")
                    }
                    idx += 1;
                }
                idx = 0;
                for core in application_cores[..*last_worker_idx].iter_mut() {
                    match core.get_action() {
                        CoreAction::Application => {
                            if idle_worker_core.is_none() && core.is_idle() {
                                // Here, we select an idle core
                                // In this case, we make sure it is not the last one
                                idle_worker_core = Some(idx);
                            }
                            match core.schedule(self.t_cur, None) {
                                CoreState::Finished(req) => {
                                    log::warn!("Worker Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                    self.finished.push_back(req);
                                    self.finished_per_core[core.get_id()] += 1;
                                },
                                _ => {} // Moving on
                            }
                        },
                        _ => panic!("ERROR: should not be here.")
                    }
                    idx += 1;
                }

                // Second, we make progress in the network core (enqueuing to ready_queue, when the processing completes)
                match network_core.schedule(self.t_cur, None) {
                    CoreState::Finished(req) => {
                        match network_core.try_enqueue_ready_queue(req) {
                            Ok(()) => {
                                if let Some(worker_idx) = idle_worker_core {
                                    self.last_workers_idx.insert(network_core.get_id(), worker_idx);
                                    let req: Request = network_core.pop_ready_queue();
                                    let worker: &mut Core = &mut application_cores[worker_idx];
                                    worker.try_enqueue(req).expect("ERROR: should not be here.")
                                }
                            },
                            Err(mut req) => {
                                req.set_p_dropped();
                                self.dropped.push_back(req);
                                self.dropped_per_core[network_core.get_id()] += 1;
                            }
                        }                        
                    },
                    _ => {} // Moving on
                }
            },
            Layout::Layout4(network_cores, map) => {
                // First, we need make progress in all worker cores
                for application_cores in map.values_mut() {
                    for core in application_cores.iter_mut() {
                        match core.get_action() {
                            CoreAction::Application => {
                                match core.schedule(self.t_cur, None) {
                                    CoreState::Finished(req) => {
                                        log::warn!("Worker Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                        self.finished.push_back(req); //TODO: verificar
                                        self.finished_per_core[core.get_id()] += 1;
                                    },
                                    _ => {} // Moving on
                                }
                            },
                            _ => panic!("ERROR: should not be here.")
                        }
                    }
                }
                // Second, for EACH network stack core, we see if there is an idle application core
                for network_core in network_cores {
                    // Second, we make progress in the network core (enqueuing to ready_queue, when the processing completes)
                    match network_core.schedule(self.t_cur, None) {
                        CoreState::Finished(req) => {
                            match network_core.try_enqueue_ready_queue(req) {
                                Ok(()) => {
                                    //TODO: verificar se pode fazer uma iteracao e depois encaminhar
                                    let idle_worker_core: Option<&mut Core> = {
                                        let mut worker_core: Option<&mut Core> = None;
                                        let n: usize = map.get(&network_core.get_id()).unwrap().len();
                                        let last_worker_idx: &usize = self.last_workers_idx.get(&network_core.get_id()).unwrap();
            
                                        for i in 0..n {
                                            let idx: usize = (*last_worker_idx + i + 1) % n;
                                            let core: &Core = &map.get(&network_core.get_id()).unwrap()[idx];
                                            if core.is_idle() {
                                                self.last_workers_idx.insert(network_core.get_id(), idx);
                                                worker_core = Some(&mut map.get_mut(&network_core.get_id()).unwrap()[idx]);
                                                break;
                                            }
                                        }
                                        worker_core
                                    };

                                    if let Some(worker_core) = idle_worker_core {
                                        let req: Request = network_core.pop_ready_queue();
                                        worker_core.try_enqueue(req).expect("ERROR: should not be here.")
                                    }
                                },
                                Err(mut req) => {
                                    req.set_p_dropped();
                                    self.dropped.push_back(req);
                                    self.dropped_per_core[network_core.get_id()] += 1;
                                }
                            }                        
                        },
                        _ => {} // Moving on
                    }
                    // TODO: tem que verificar a volta da aplicacao para o network stack core... como fazer essa volta?
                }
            }
        }
    }

    pub fn run(&mut self) {
        println!("\nRunning the simulator...");

        // Set the current time of the simulator as the arrival time of the first request.
        self.t_cur = self.packets.get(0).expect("Should be at least one request").get_arrival_time();
        self.progress_bar.inc(self.t_cur as u64);

        // Array for the incoming requests.
        let mut received_requests: Vec<Request> = Vec::<Request>::new();

        // We run till the duration and have remaining requests to be processed.
        while self.t_cur < self.t_duration && self.has_remaining_requests() {
            // Schedule all cores to make progress.
            self.schedule_all_cores();

            // Check for new incoming requests.
            if !self.packets.is_empty() {
                let next_arrival_time: usize = (&self.packets[0]).get_arrival_time();
                if self.t_cur == next_arrival_time {
                    // Check for requests that arrived in the same time.
                    while !self.packets.is_empty() {
                        if next_arrival_time == (&self.packets[0]).get_arrival_time() {
                            let req: Request = self.packets.pop_front().unwrap();
                            received_requests.push(req);
                            self.received += 1;
                        } else {
                            break;
                        }
                    }
                }
            }

            // Enqueue the incoming requests received at time 't_cur' to the cores
            while let Some(req) = received_requests.pop() {
                let core: &mut Core = self.select_core(&req);
                let core_id: usize = core.get_id();
                match core.try_enqueue(req) {
                    Ok(()) => {},
                    Err(mut req) => {
                        req.set_p_dropped();
                        self.dropped.push_back(req);
                        self.dropped_per_core[core_id] += 1;
                    }
                }
            }

            // Move ticks forward.
            self.t_cur += 1;
            self.progress_bar.inc(1);
        }

        self.progress_bar.finish();
        println!("Done.");
    }

    fn print_raw(&self) {
        let filename: String = format!("layout{:?}_run{:?}.dat", self.layout, self.run_id);

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(filename.as_str()).unwrap();

        for req in &self.finished {
            let t: usize = req.get_departure_time() - req.get_arrival_time() + self.rtt_base;
            let _ = std::io::Write::write_fmt(&mut file, format_args!("{}\n", t));
        }
    }

    fn print_stats(&self) {
        let filename: String = format!("layout{:?}_run{:?}.csv", self.layout, self.run_id);
        let mut writer: csv::Writer<File> = csv::Writer::from_path(filename.as_str()).unwrap();
        
        // writer.write_record(&["#total requests", "total dropped requests", "total completed requests", "min", "p25", "p50", "p75", "p99.9", "p99.99", "max", "finished/dropped per core..."]).unwrap();

        let total_requests = self.nr_packets;
        let dropped_requests: usize = self.dropped.len();
        let completed_requests: usize = self.finished.len();

        let mut arr: Vec<usize> = Vec::<usize>::new();
        for i in &self.finished {
            arr.push(i.get_departure_time() - i.get_arrival_time());
        }
        arr.sort();

        let mut row: Vec<usize> = Vec::<usize>::new();
        row.push(total_requests);
        row.push(self.received);
        row.push(completed_requests);
        row.push(dropped_requests);
        row.push(percentiles(&arr, 0.0));
        row.push(percentiles(&arr, 25.0));
        row.push(percentiles(&arr, 50.0));
        row.push(percentiles(&arr, 75.0));
        row.push(percentiles(&arr, 99.9));
        row.push(percentiles(&arr, 99.99));
        row.push(percentiles(&arr, 100.0));

        for i in 0..self.nr_total_cores {
            row.push(self.finished_per_core[i]);
            row.push(self.dropped_per_core[i]);
        }

        writer.serialize(row).unwrap();
        writer.flush().unwrap();
        drop(writer);
    }
}

fn percentiles(arr: &Vec<usize>, p: f64) -> usize {
    let mut idx: usize = (arr.len() as f64 * (p/100.0)) as usize;
    if idx == arr.len() {
        idx -= 1;
    }
    arr[idx]
}

fn main() {
    let rng: Rc<RefCell<SmallRng>> = Rc::new(RefCell::new(SmallRng::seed_from_u64(INITIAL_SEED)));
    for i in 0..1 {
        let mut sim: Simulation = Simulation::new(i, rng.clone());
        sim.run();
        sim.print_stats();
        sim.print_raw();
    }
}