use std::thread;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;
use std::{cmp::min, fmt::Write};
use indicatif::{
    ProgressBar, 
    ProgressState, 
    ProgressStyle
};
use log::{info, warn};

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

enum QueueDiscipline {
    cFCFS,
    dFCFS,
}

pub enum Phase {
    Phase1,
    Phase2,
    Phase3,
    Phase4,
}

struct Simulation {
    // NIC Related
    nr_table_entries: usize,                        // The number of indirection table entries
    indirection_table: HashMap<usize, usize>,       // Indirection table to map queue_id->core_id

    // Server Related
    nr_cores_1phase: usize,                         // The number of cores of phase 1 (only grab packets from the NIC)
    nr_cores_2phase: usize,                         // The number of cores of phase 2 (only process the network stack)
    nr_cores_3phase: usize,                         // The number of cores of phase 3 (only process the application)
    nr_cores_4phase: usize,                         // The number of cores of phase 3 (process the network stack and the application)
    siblings_table: HashMap<usize, usize>,          // The sibling table for hyperthreading
    phases_table: HashMap<usize, usize>,            // The phase table to map core_id->phase
    queue_discipline: QueueDiscipline,              // Queue Discipline


    // Request Related

    // Simulator Related
    t_cur: usize,                                   // Current time (in ticks)
    t_duration: usize,                              // Duration of the simulation (in ticks)
    nr_requests_completed: usize,                   // Number of requests completed
    dropped: VecDeque<Request>,                     // Array of all dropped requests
    cores: Vec<Core>,                               // Array of all simulated cores
    requests: VecDeque<Request>,                    // Array of all requests to be run into Simulator
    progress_bar: ProgressBar,                      // Progress bar
}

impl Simulation {
    pub fn new() -> Simulation {
        info!("Creating the Simulation");

        let t_duration: usize = 10000000;
        let nr_queues = 8;
        let queue_size = 1024;
        let requests = Self::configure_requests();

        let rss_entries = 128;
        let nr_cores = 4;
        let queue_discipline = QueueDiscipline::cFCFS;

        let progress_bar: ProgressBar = ProgressBar::new(t_duration as u64);
        progress_bar.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:40.blue/white}] {pos:>7}/{len:7} (ticks) [{percent}]")
            .unwrap()
            .with_key("percent", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.2}%", (state.fraction() * 100.0)).unwrap())
            .progress_chars("#>-"));

        // Assuming Round-Robin for the queue_id <-> core_id relationship
        let nr_table_entries = 128;
        let mut indirection_table = HashMap::<usize,usize>::new();

        let nr_cores_1phase = 0;
        let nr_cores_2phase = 0;
        let nr_cores_3phase = 0;
        let nr_cores_4phase = 8;

        // Sibling table for hyperthreading
        let mut siblings_table = HashMap::<usize,usize>::new();

        // Array for arrays
        let mut cores = Vec::<Core>::new();
        for core_id in 0..nr_cores {
            let core = Core::new(core_id, Phase::Phase4, queue_size);
            cores.push(core);
        }

        // CPU phases for core
        let phases_table = HashMap::<usize,usize>::new();

        // Cores phase1 is only to get packet from the NIC
        let nr_cores_phase1 = match queue_discipline {
            QueueDiscipline::cFCFS => 1,
            QueueDiscipline::dFCFS => nr_cores,
        };

        // TODO: could be simulate a real RSS
        for queue_id in 0..nr_cores_phase1 {
            indirection_table.insert(queue_id, queue_id % nr_cores_phase1);
        }
        
        let sim = Simulation {
            // NIC Related
            nr_table_entries,
            indirection_table,

            // Server Related
            nr_cores_1phase,
            nr_cores_2phase,
            nr_cores_3phase,
            nr_cores_4phase,
            siblings_table,
            phases_table,
            queue_discipline,

            // Simulator Related
            t_cur: 0,
            t_duration,
            nr_requests_completed: 0,
            dropped: VecDeque::<Request>::new(),
            cores,
            requests,
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
                0,
                0,
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
            req.t_arrival -= first;
            req.set_id(i);
            i += 1;
        }

        let requests = VecDeque::from(tasks);

        pb.finish();
        println!("Done.");

        requests
    }

    fn has_unprocessing_requests(&self) -> bool {
        self.nr_requests_completed < self.requests.len()
    }

    pub fn enqueue_incoming_request(&mut self, req: Request) {
        let core_id = match self.queue_discipline {
            QueueDiscipline::cFCFS => {
                // Centralized First-Come-First-Serve
                0
            }
            QueueDiscipline::dFCFS => {
                // Distributed First-Come-First-Serve
                let queue_id: usize = req.get_flow_id() % self.nr_table_entries;
                let core_id: usize = *self.indirection_table.get(&queue_id).unwrap();
                core_id
            }
        };

        match self.cores[core_id].try_enqueue(req) {
            Ok(()) => {},
            Err(mut req) => {
                req.set_dropped();
                self.dropped.push_back(req);
            }
        }
    }

    pub fn run(&mut self) {
        println!("\nRunning the simulator...");

        // Set the current time of the simulator as the arrival time of the first request.
        self.t_cur = self.requests.get(0).expect("Should be at least one request").get_arrival_time();
        self.progress_bar.inc(self.t_cur as u64);

        // Next request.
        let mut next_request: usize = 0;
        
        // Array for requests that received in the same time.
        let mut received_requests: Vec<Request> = Vec::<Request>::new();

        // Array for requests that finished.
        let mut finished_requests: Vec<usize> = Vec::<usize>::new();

        // Array for requests that are running.
        let mut enqueued_requests: Vec<usize> = Vec::<usize>::new();

        // We run till duration
        while self.t_cur < self.t_duration {

            // Schedule all cores
            for core in &mut self.cores {
                core.schedule(self.t_cur);
            }

            // Check for new incoming requests.
            if !self.requests.is_empty() {
                let next_arrival_time: usize = (&self.requests[0]).get_arrival_time();
                if self.t_cur == next_arrival_time {
                    // Check for requests that arrived in the same time.
                    while !self.requests.is_empty() {
                        if next_arrival_time == (&self.requests[0]).get_arrival_time() {
                            let req = self.requests.pop_front().unwrap();
                            received_requests.push(req);
                        } else {
                            break;
                        }
                    }
                }
            }

            // Enqueue the incoming requests
            while let Some(req) = received_requests.pop() {
                self.enqueue_incoming_request(req);
            }

            // Move ticks forward.
            self.t_cur += 1;
            self.progress_bar.inc(1);
        }

        self.progress_bar.finish();
        println!("Done.")
    }
}

// We assume that the same interarrival can be 

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
