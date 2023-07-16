extern crate rustc_serialize;
use rustc_serialize::json::Json;

mod request;
use request::Request;

mod worker_core;
use worker_core::Core;

use ::std::{
    fs::File,
    io::Read,
    fmt::Write,
    collections::{
        HashMap,
        VecDeque,
    },
};
use ::rand::{
    rngs::SmallRng,
    RngCore,
    SeedableRng,
};
use indicatif::{
    ProgressBar, 
    ProgressState, 
    ProgressStyle
};
use log::info;
use arrayvec::ArrayVec;

enum Layout {
    Layout1(Core, Vec<Core>, Vec<usize>),
    Layout2(Vec<Core>),
    Layout3(Core, Vec<Core>),
    Layout4(Vec<Core>, HashMap<usize, Vec<Core>>),
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
    nr_indirection_table_entries: usize,            // The number of indirection table entries
    indirection_table: HashMap<usize, usize>,       // Indirection table to map queue_id->core_id

    // Server Related
    layout: Layout,                                 // Which layout the simulator will run

    // Simulator Related
    t_cur: usize,                                   // Current time (in ticks)
    t_duration: usize,                              // Duration of the simulation (in ticks)
    last_workers_idx: HashMap<usize, usize>,                         // The index of last worker core used
    dropped: VecDeque<Request>,                     // Array of all dropped requests
    packets: VecDeque<Request>,                     // Array of all requests to be run into Simulator
    progress_bar: ProgressBar,                      // Progress bar
}

impl Simulation {
    pub fn new() -> Simulation {
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
                    println!("Mapping: {:?} -> {:?}", network_id, core_id);
                    let arr: &mut Vec<Core> = map.get_mut(&network_id).unwrap();
                    arr.push(core);
                }

                Layout::Layout4(arr, map)
            },
            _ => panic!("ERROR: layout should be 1, 2, 3, or 4.")
        };

        let packets = {
            let seed: u64 = json.find(&"seed").unwrap().as_u64().unwrap();
            let mut rng = SmallRng::seed_from_u64(seed);

            let nr_packets: usize = json.find_path(&["packets", "nr_packets"]).unwrap().as_u64().unwrap() as usize;

            let mut packets = Vec::<Request>::with_capacity(nr_packets);

            println!("\nConfiguring the requests...");
            let pb: ProgressBar = ProgressBar::new(nr_packets as u64);
            pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:40.green/white}] {pos:>7}/{len:7} (requests)")
            .unwrap()
            .progress_chars("#>-"));

            let interarrival_distribution: &str = json.find_path(&["packets", "distribution"]).unwrap().as_string().unwrap();
            let arrival_mean1: usize = json.find_path(&["packets", "mean1"]).unwrap().as_u64().unwrap() as usize;
            let stack_distribution: &str = json.find_path(&["network_stack", "distribution"]).unwrap().as_string().unwrap();
            let stack_mean1: usize = json.find_path(&["network_stack", "mean1"]).unwrap().as_u64().unwrap() as usize;
            let application_distribution: &str = json.find_path(&["application", "distribution"]).unwrap().as_string().unwrap();
            let application_mean1: usize = json.find_path(&["application", "mean1"]).unwrap().as_u64().unwrap() as usize;

            let mut last_t_arrival: usize = 0;
            for i in 0..nr_packets {
                last_t_arrival = match interarrival_distribution {
                    "constant" => last_t_arrival + arrival_mean1,
                    _ => 100,
                };

                let last_stack_time: usize = match stack_distribution {
                    "constant" => stack_mean1,
                    _ => 1,
                };

                let last_application_time: usize = match application_distribution {
                    "constant" => application_mean1,
                    _ => 1,
                };

                let req = Request::new(
                    i as usize,
                    (rng.next_u64() % nr_flows) as usize,
                    last_t_arrival,
                    last_stack_time,
                    last_application_time,
                );

                packets.push(req);
                pb.inc(1);
            }

            // Order the packets according to arrival_time
            packets.sort_by(|a, b| a.get_arrival_time().cmp(&b.get_arrival_time()));

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

        let sim: Simulation = Simulation {
            // NIC Related
            nr_indirection_table_entries,
            indirection_table,

            // Server Related
            layout,

            // Simulator Related
            t_cur: 0,
            t_duration,
            last_workers_idx,
            dropped: VecDeque::<Request>::new(),
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

    pub fn run(&mut self) {
        println!("\nRunning the simulator...");

        // Set the current time of the simulator as the arrival time of the first request.
        self.t_cur = self.packets.get(0).expect("Should be at least one request").get_arrival_time();
        self.progress_bar.inc(self.t_cur as u64);

        // Array for requests that received in the same time.
        let mut received_requests: Vec<Request> = Vec::<Request>::new();

        // Array for requests that finished.
        let mut finished_requests: Vec<Request> = Vec::<Request>::new();

        //DEBUG
        let mut per_core = ArrayVec::<usize, 16>::new();
        for _ in 0..16 {
            per_core.push(0);
        }

        // We run till duration
        // Or FINISHED + DROPPED == TOTAL
        while self.t_cur < self.t_duration {
            // Schedule all cores
            match &mut self.layout {
                Layout::Layout1(forwarder, worker_cores, locks) => {
                    // First, we need make progress in all worker cores
                    for core in worker_cores.iter_mut() {
                        match core.get_action() {
                            CoreAction::NetworkStackAndApplicationLock => {
                                match core.schedule(self.t_cur, Some(locks)) {
                                    CoreState::Finished(req) => {
                                        log::warn!("Worker Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                        finished_requests.push(req);
                                        per_core[core.get_id()] += 1; //DEBUG
                                    },
                                    _ => {} // Moving on
                                }
                            },
                            _ => panic!("ERROR: should not be here.")
                        }
                    }
                    // Second, we check if there is an idle worker
                    // TODO: we could get from above
                    let idle_worker_core: Option<&mut Core> = {
                        let mut worker_core: Option<&mut Core> = None;
                        let n: usize = worker_cores.len();
                        for i in 0..n {
                            let last_worker_idx = self.last_workers_idx.get(&forwarder.get_id()).unwrap();
                            let idx: usize = (*last_worker_idx + i + 1) % n;
                            let core: &Core = &worker_cores[idx];
                            if core.is_idle() {
                                self.last_workers_idx.insert(forwarder.get_id(), idx);
                                worker_core = Some(&mut worker_cores[idx]);
                                break;
                            }
                        }
                        worker_core
                    };
                    // Third, we forward a packet to an idle worker (if any) [WE FORWARD ONE PACKET PER TICK]
                    if let Some(worker) = idle_worker_core {
                        match forwarder.schedule(self.t_cur, None) {
                            CoreState::Finished(req) => {
                                log::warn!("Forwarded Core #{:?} finished the Request #{:?}", forwarder.get_id(), req.get_id());
                                worker.try_enqueue(req).expect("ERROR: should not be here.");
                            },
                            _ => {} // Moving on
                        }
                    }
                },
                Layout::Layout2(worker_cores) => {
                    // First, we just make progress in all worker cores
                    for core in worker_cores {
                        match core.get_action() {
                            CoreAction::NetworkStackAndApplication => {
                                match core.schedule(self.t_cur, None) {
                                    CoreState::Finished(req) => {
                                        log::warn!("Worker Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                        finished_requests.push(req);
                                        per_core[core.get_id()] += 1; //DEBUG
                                    },
                                    _ => {} // Moving on
                                }
                            },
                            _ => panic!("ERROR: should not be here.")
                        }
                    }
                },
                Layout::Layout3(network_core, application_cores) => {
                    // First, we need make progress in all worker cores
                    for core in application_cores.iter_mut() {
                        match core.get_action() {
                            CoreAction::Application => {
                                match core.schedule(self.t_cur, None) {
                                    CoreState::Finished(req) => {
                                        log::warn!("Worker Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                        finished_requests.push(req); //TODO: verificar
                                        per_core[core.get_id()] += 1; //DEBUG
                                    },
                                    _ => {} // Moving on
                                }
                            },
                            _ => panic!("ERROR: should not be here.")
                        }
                    }
                    // Second, we make progress in the network core (enqueuing to ready_queue, when the processing completes)
                    match network_core.schedule(self.t_cur, None) {
                        CoreState::Finished(req) => {
                            match network_core.try_enqueue_ready_queue(req) {
                                Ok(()) => {
                                    //TODO: verificar se pode fazer uma iteracao e depois encaminhar
                                    let idle_worker_core: Option<&mut Core> = {
                                        let mut worker_core: Option<&mut Core> = None;
                                        let n: usize = application_cores.len();
                                        let last_worker_idx: &usize = self.last_workers_idx.get(&network_core.get_id()).unwrap();
                                        for i in 0..n {
                                            let idx: usize = (*last_worker_idx + i + 1) % n;
                                            let core: &Core = &application_cores[idx];
                                            if core.is_idle() {
                                                self.last_workers_idx.insert(network_core.get_id(), idx);
                                                worker_core = Some(&mut application_cores[idx]);
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
                                }
                            }                        
                        },
                        _ => {} // Moving on
                    }
                    // TODO: tem que verificar a volta da aplicacao para o network stack core... como fazer essa volta?
                },
                Layout::Layout4(network_cores, map) => {
                    // First, we need make progress in all worker cores
                    for application_cores in map.values_mut() {
                        for core in application_cores.iter_mut() {
                            match core.get_action() {
                                CoreAction::Application => {
                                    match core.schedule(self.t_cur, None) {
                                        CoreState::Finished(req) => {
                                            // log::warn!("Worker Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                            // println!("Worker Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                            finished_requests.push(req); //TODO: verificar
                                            per_core[core.get_id()] += 1; //DEBUG
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
                                    }
                                }                        
                            },
                            _ => {} // Moving on
                        }
                        // TODO: tem que verificar a volta da aplicacao para o network stack core... como fazer essa volta?
                    }
                }
            }

            // Check for new incoming requests.
            if !self.packets.is_empty() {
                let next_arrival_time: usize = (&self.packets[0]).get_arrival_time();
                if self.t_cur == next_arrival_time {
                    // Check for requests that arrived in the same time.
                    while !self.packets.is_empty() {
                        if next_arrival_time == (&self.packets[0]).get_arrival_time() {
                            let req = self.packets.pop_front().unwrap();
                            received_requests.push(req);
                        } else {
                            break;
                        }
                    }
                }
            }

            // Enqueue the incoming requests received at time 't_cur' to the cores
            while let Some(req) = received_requests.pop() {
                let core: &mut Core = self.select_core(&req);
                match core.try_enqueue(req) {
                    Ok(()) => {},
                    Err(mut req) => {
                        req.set_p_dropped();
                        self.dropped.push_back(req);
                    }
                }
            }

            // Move ticks forward.
            self.t_cur += 1;
            self.progress_bar.inc(1);
        }

        self.progress_bar.finish();
        println!("Done.");

        for i in 0..8 {
            println!("Core #{:?} = {:?} requests", i, per_core[i])
        }
        println!("Dropped: {:?}", self.dropped.len());
    }
}

fn main() {
    let mut sim: Simulation = Simulation::new();


    //requests should be > 0

    // pb.finish_with_message("downloaded");

    sim.run();


    // A saida tem que ter:
    // - quantidade de requisicoes
    // - quantidade de requisicoes que finalizaram
    // - que nao finalizaram
    // - que nao chegaam
    // - que droparam na fila do nic
    // - que droparam na fila do core
    // 
    // 
}


// O simulador tem que receber qual layout ele vai avaliar
// Layout1..4
// Para cada layout ele tem que receber a quantidade de core

// Cada core tem que saber o que ele deve executar
// Apenas encaminhar (layout 1)
// Processar pilha de protocolo + applicacao (layout 1,d2)
// Pilha de procolo e encaminhar (layout 3,4)
// 