// Created by Fabricio Carvalho (fabricio.carvalho@ufmt.br)

// Imports


// Quick Explanation of this

// Structure
pub struct Request {
    id: usize,
    flow_id: usize,
    pub t_arrival: usize,
    t_start: usize,
    t_end: usize,
    t_completed: usize,
    // type_of_task: usize, // Not used for now
    service_time: usize,
    //
    is_completed: bool,
    is_dropped: bool,
}

// Associate Functions
impl Request {
    pub fn new(
        id: usize,
        flow_id: usize,
        t_arrival: usize,
        t_start: usize,
        t_end: usize,
        service_time: usize,
    ) -> Request {
        let req = Request {
            id,
            flow_id,
            t_arrival,
            t_start,
            t_end,
            t_completed: 0,
            service_time,
            is_completed: false,
            is_dropped: false,
        };

        req
    }

    pub fn set_id(&mut self, id: usize) {
        self.id = id;
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn get_flow_id(&self) -> usize {
        self.flow_id
    }

    pub fn get_arrival_time(&self) -> usize {
        self.t_arrival
    }
    
    pub fn get_service_time(&self) -> usize {
        self.service_time
    }

    pub fn is_completed(&self) -> bool {
        self.is_completed
    }

    pub fn set_start(&mut self, t_cur: usize) {
        self.t_start = t_cur;
    }

    pub fn set_dropped(&mut self) {
        self.is_dropped = true;
    }

    pub fn schedule(&mut self) {
        self.t_completed += 1;
        if self.t_completed == self.service_time {
            self.is_completed = true;
        }
    }
}