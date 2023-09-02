extern crate rustc_serialize;
use rustc_serialize::json::Json;
use std::fs::File;
use std::io::Read;

use std::cell::{RefCell, RefMut, Ref};
use std::collections::{HashMap, VecDeque};
use std::fmt::Write;
use std::rc::Rc;
use indicatif::{
    ProgressBar, 
    ProgressState, 
    ProgressStyle
};
use log::info;

use ::rand::{
    rngs::SmallRng,
    RngCore,
    SeedableRng,
};

mod request;
use request::Request;

mod worker_core;
use worker_core::Core;

use arrayvec::ArrayVec;

const MAX_REQUESTS_RECEIVED: usize = 32;

enum Layout {
    Layout1(&'static mut Core, Vec<&'static mut Core>),     // Index of first core to receive the packet from the NIC and Vector containing indexes of which core to process
    Layout2(Vec<usize>),            // Vector containing indexes of which core to process
    Layout3(usize, Vec<usize>),     // Index for network stack and Vector for workers
    Layout4(Vec<usize>, Vec<usize>),    // Vector for network stack and Vector for application
}

pub enum CoreAction {
    Forward,
    Application,
    NetworkStack,
    NetworkStackAndApplication,
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

    // Request Related

    // Simulator Related
    t_cur: usize,                                   // Current time (in ticks)
    t_duration: usize,                              // Duration of the simulation (in ticks)
    nr_requests_completed: usize,                   // Number of requests completed
    last_application_idx: usize,                    // The index of last application core used
    last_worker_idx: usize,                         // The index of last worker core used
    dropped: VecDeque<Request>,                     // Array of all dropped requests
    packets: VecDeque<Request>,                    // Array of all requests to be run into Simulator
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

        let mut cores: Vec<Core> = Vec::<Core>::with_capacity(nr_total_cores);

        let which_layout: usize = json.find(&"layout").unwrap().as_u64().unwrap() as usize;
        let layout = match which_layout {
            1 => {
                let nr_worker_cores: usize = json.find_path(&["layout1", "nr_worker_cores"]).unwrap().as_u64().unwrap() as usize;
                if nr_worker_cores + 1 > nr_total_cores {
                    panic!("ERROR: the number of cores");
                }

                let mut forwarder: Core = Core::new(0, CoreAction::Forward, queue_size);
                let forwarder_ptr: &mut Core = &mut forwarder;
                cores.push(forwarder);

                let mut arr: Vec<&mut Core> = Vec::<&mut Core>::with_capacity(nr_worker_cores);
                for i in 0..nr_worker_cores {
                    let mut core: Core = Core::new(i + 1, CoreAction::NetworkStackAndApplication, queue_size);
                    arr.push(&mut core);
                    cores.push(core);
                }

                Layout::Layout1(forwarder_ptr, arr)
            },
            2 => {
                let nr_worker_cores: usize = json.find_path(&["layout2", "nr_worker_cores"]).unwrap().as_u64().unwrap() as usize;
                if nr_worker_cores > nr_total_cores {
                    panic!("ERROR: the number of cores");
                }

                let mut arr: Vec<usize> = Vec::<usize>::with_capacity(nr_worker_cores);
                for i in 0..nr_worker_cores {
                    arr.push(i);
                    let core: Core = Core::new(i, CoreAction::NetworkStackAndApplication, queue_size);
                    cores.push(core);
                }

                Layout::Layout2(arr)
            },
            3 => {
                let nr_application_cores: usize = json.find_path(&["layout3", "nr_application_cores"]).unwrap().as_u64().unwrap() as usize;

                if nr_application_cores + 1 > nr_total_cores {
                    panic!("ERROR: the number of cores");
                }

                let network_core: Core = Core::new(0, CoreAction::NetworkStack, queue_size);
                cores.push(network_core);

                let mut arr: Vec<usize> = Vec::<usize>::with_capacity(nr_application_cores);
                for i in 0..nr_application_cores {
                    arr.push(i + 1);
                    let core: Core = Core::new(i + 1, CoreAction::Application, queue_size);
                    cores.push(core)
                }

                Layout::Layout3(0, arr)
            },
            4 => {
                let nr_network_cores: usize = json.find_path(&["layout4", "nr_network_cores"]).unwrap().as_u64().unwrap() as usize;
                let nr_application_cores: usize = json.find_path(&["layout4", "nr_application_cores"]).unwrap().as_u64().unwrap() as usize;

                if nr_network_cores + nr_application_cores > nr_total_cores {
                    panic!("ERROR: the number of cores");
                }

                let mut arr0: Vec<usize> = Vec::<usize>::with_capacity(nr_network_cores);
                for i in 0..nr_network_cores {
                    arr0.push(i);
                    let core: Core = Core::new(i, CoreAction::NetworkStack, queue_size);
                    cores.push(core);
                }

                let mut arr1: Vec<usize> = Vec::<usize>::with_capacity(nr_application_cores);
                for i in 0..nr_application_cores {
                    arr1.push(nr_network_cores + i);
                    let core: Core = Core::new(nr_network_cores + i, CoreAction::Application, queue_size);
                    cores.push(core);
                }

                Layout::Layout4(arr0, arr1)
            },
            _ => panic!("ERROR: layout should be 1, 2, 3, or 4.")
        };

        let packets = {
            // We need to configure:
            // - interarrival of the packets
            // - network stack time
            // - application time

            
            let seed: u64 = json.find(&"seed").unwrap().as_u64().unwrap();
            let mut rng = SmallRng::seed_from_u64(seed);

            let nr_packets: usize = json.find_path(&["packets", "nr_packets"]).unwrap().as_u64().unwrap() as usize;
            let nr_flows: u64 = json.find_path(&["packets", "nr_flows"]).unwrap().as_u64().unwrap();

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
            let mut last_stack_time: usize = 0;
            let mut last_application_time: usize = 0;
            for i in 0..nr_packets {
                last_t_arrival = match interarrival_distribution {
                    "constant" => arrival_mean1,
                    _ => 100,
                };

                last_stack_time = match stack_distribution {
                    "constant" => stack_mean1,
                    _ => 1,
                };

                last_application_time = match application_distribution {
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
            Layout::Layout1(_, _) => 1,
            Layout::Layout2(arr) => arr.len(),
            Layout::Layout3(_, _) => 1,
            Layout::Layout4(arr, _) => arr.len(),
        };

        let nr_indirection_table_entries: usize = json.find(&"nr_indirection_table_entries").unwrap().as_u64().unwrap() as usize;
        for i in 0..nr_indirection_table_entries {
            indirection_table.insert(i, i % nr_queues);
        }

        let sim = Simulation {
            // NIC Related
            nr_indirection_table_entries,
            indirection_table,

            // Server Related
            layout,

            // Simulator Related
            t_cur: 0,
            t_duration,
            nr_requests_completed: 0,
            last_application_idx: 0,
            last_worker_idx: 0,
            dropped: VecDeque::<Request>::new(),
            cores: Rc::new(RefCell::new(cores)),
            packets,
            progress_bar
        };

        sim
    }

    fn configure_requests() -> VecDeque<Request> {
        let total_requests = 100_000;
        let total_flows: u64 = 128;
        // Read and configure all requests
        // We should have a list of requests ordered by arrival time
        let mut tasks = Vec::<Request>::with_capacity(total_requests as usize);

        println!("\nConfiguring the requests...");

        let pb: ProgressBar = ProgressBar::new(total_requests);
        pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:40.green/white}] {pos:>7}/{len:7} (requests)")
            .unwrap()
            .progress_chars("#>-"));

        //TEMP
        let mut rng = SmallRng::seed_from_u64(3);
        for i in 0..total_requests {
            let req = Request::new(
                i as usize,
                (rng.next_u64() % total_flows) as usize,
                (rng.next_u64() % 1000000) as usize,
                10,
                (rng.next_u64() % 1000) as usize,
            );
            tasks.push(req);
            pb.inc(1);
        }

        // set numer of flow

        tasks.sort_by(|a, b| a.get_arrival_time().cmp(&b.get_arrival_time()));
        tasks[0].set_id(0);
        //TEMP
        let mut i = 1;
        let first = &tasks[0].get_arrival_time();
        for req in &mut tasks[1..] {
            req.set_arrival_time(req.get_arrival_time() - first);
            req.set_id(i);
            i += 1;
        }

        let requests = VecDeque::from(tasks);

        pb.finish();
        println!("Done.");

        requests
    }

    fn select_core(&self, req: &Request) -> usize {
        let queue_id: usize = req.get_flow_id() % self.nr_indirection_table_entries;
        let core_id: usize = *self.indirection_table.get(&queue_id).unwrap();
        core_id
    }

    fn select_worker_core(&self) -> Option<usize> {
        match &self.layout {
            Layout::Layout1(_, worker_cores) => {
                for i in 0..worker_cores.len() {
                    let worker_idx: usize = (self.last_worker_idx + i + 1) % worker_cores.len();
                    if self.cores.borrow()[worker_idx].is_idle() {
                        return Some(worker_idx)
                    }
                }
                // If reach here, it means that there aren't idle worker cores
                None
            },
            _ => None,
        }
    }

    fn select_application_core(&self) -> Option<usize> {
        match &self.layout {
            Layout::Layout3(_, application_cores) => {
                let cores_borrowed: std::cell::Ref<Vec<Core>> = self.cores.borrow();
                for i in 0..application_cores.len() {
                    let application_idx: usize = (self.last_application_idx + i + 1) % application_cores.len();
                    if cores_borrowed[application_idx].is_idle() {
                        return Some(application_idx)
                    }
                }

                // If reach here, it means that there aren't idle application cores
                None
            },
            _ => None
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

        let mut cores_borrowed: RefMut<Vec<Core>> = self.cores.borrow_mut();

        // We run till duration
        while self.t_cur < self.t_duration {
            // Schedule all cores
            match &self.layout {
                Layout::Layout1(forwarder, worker_cores) => {

                },
                // Layout::Layout1(forwarded_idx, worker_cores_idx) => {
                //     // Forwarded core
                //     let core: &mut Core = &mut cores_borrowed[*forwarded_idx];
                //     match core.get_action() {
                //         CoreAction::Forward => {
                //             // First, we need to check if there is an idle worker
                //             let selecting_worker_core: Option<usize> = {
                //                 let mut selected_worker_core: Option<usize> = None;
                //                 for i in worker_cores_idx.iter() {
                //                     let worker_core: &Core = &cores_borrowed[*i];
                //                     if worker_core.is_idle() {
                //                         selected_worker_core = Some(*i);
                //                         break;
                //                     }
                //                 }
                //                 selected_worker_core
                //             };

                //             if let Some(worker_idx) = selecting_worker_core {
                //                 match core.schedule(self.t_cur) {
                //                     CoreState::Finished(req) => {
                //                         log::warn!("Forwarded Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                //                         self.last_worker_idx = worker_idx;
                //                         cores_borrowed[worker_idx].try_enqueue(req).expect("ERROR: should not be here.");
                //                     },
                //                     _ => {} // Moving on
                //                 }
                //             };
                //         },
                //         _ => panic!("ERROR: should not be here.")
                //     }

                //     // Worker cores
                //     for i in worker_cores_idx.iter() {
                //         let core: &mut Core = &mut cores_borrowed[*i];
                //         match core.get_action() {
                //             CoreAction::NetworkStackAndApplication => {
                //                 match core.schedule(self.t_cur) {
                //                     CoreState::Finished(req) => {
                //                         // Layout 1
                //                         log::warn!("Worker Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                //                         finished_requests.push(req);
                //                         per_core[core.get_id()] += 1; //DEBUG
                //                     },
                //                     _ => {} // Moving on
                //                 }
                //             },
                //             _ => panic!("ERROR: should not be here.")
                //         }
                //     }
                // },
                Layout::Layout2(worker_cores_idx) => {
                    for i in worker_cores_idx.iter() {
                        let core: &mut Core = &mut cores_borrowed[*i];
                        match core.get_action() {
                            CoreAction::NetworkStackAndApplication => {
                                match core.schedule(self.t_cur) {
                                    CoreState::Finished(req) => {
                                        // Layout 2
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
                Layout::Layout3(network_stack_idx, application_cores_idx) => {
                    // Network stack core
                    let core: &mut Core = &mut cores_borrowed[*network_stack_idx];
                    match core.get_action() {
                        CoreAction::NetworkStack => {
                            match core.schedule(self.t_cur) {
                                CoreState::Finished(req) => {
                                    // Layout 3
                                    log::warn!("Network stack Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                    match self.select_application_core() {
                                        Some(application_idx) => {
                                            self.last_application_idx = application_idx;
                                            cores_borrowed[application_idx].try_enqueue(req).expect("ERROR: should not be here.");
                                        },
                                        None => {
                                            // No idle application core available
                                            match core.try_enqueue_ready_queue(req) {
                                                Ok(_) => {}
                                                Err(mut req) => {
                                                    req.set_p_dropped();
                                                    self.dropped.push_back(req);
                                                }
                                            }
                                        }
                                    }
                                },
                                _ => {}
                            }
                        },
                        _ => panic!("ERROR: should not be here.")
                    }
                    
                    // Application cores
                    for i in application_cores_idx.iter() {
                        let core: &mut Core = &mut cores_borrowed[*i];
                        match core.get_action() {
                            CoreAction::Application => {
                                match core.schedule(self.t_cur) {
                                    CoreState::Idle => todo!(),
                                    CoreState::Running => todo!(),
                                    CoreState::Finished(req) => {
                                        // Layout 3
                                        log::warn!("Application Core #{:?} finished the Request #{:?}", core.get_id(), req.get_id());
                                        finished_requests.push(req);
                                        per_core[core.get_id()] += 1; //DEBUG
                                    }
                                }
                            },
                            _ => panic!("ERROR: should not be here.")
                        }
                    }

                    // TODO: tem que verificar a volta da aplicacao para o network stack core... como fazer essa volta?
                },
                Layout::Layout4(_, _) => todo!(),
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
                let core_id: usize = self.select_core(&req);
                match cores_borrowed[core_id].try_enqueue(req) {
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

        for i in 0..4 {
            println!("Core #{:?} = {:?} requests", i, per_core[i])
        }
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