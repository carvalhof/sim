// Created by Fabricio Carvalho (fabricio.carvalho@ufmt.br)

// Imports

use std::fmt;


// Quick Explanation of this
// Each requestion has:
// t_arrived: time of this request reached in the server
// t_start: time of the request started
// 
// Structure
pub struct Request {
    // General information
    id: usize,
    t_arrival: usize,
    t_departure: usize,
    is_p_dropped: bool,
    is_r_dropped: bool,
    is_p_completed: bool,
    is_r_completed: bool,
    // Forwarder information
    t_forwarded: usize,     // Only for Layout1
    // Packet infomation
    flow_id: usize,
    t_p_start: usize,
    t_p_end: usize,
    p_completed: usize,
    stack_time: usize,
    // Request information
    t_r_start: usize,
    t_r_end: usize,
    r_completed: usize,
    request_time: usize,
}

// Associate Functions
impl Request {
    pub fn new(
        id: usize,
        flow_id: usize,
        t_arrival: usize,
        stack_time: usize,
        request_time: usize,
    ) -> Request {
        let req = Request {
            id,
            t_arrival,
            t_departure: 0,
            is_p_dropped: false,
            is_p_completed: false,
            is_r_dropped: false,
            is_r_completed: false,
            t_forwarded: 0,
            flow_id,
            t_p_start: 0,
            t_p_end: 0,
            p_completed: 0,
            stack_time,
            t_r_start: 0,
            t_r_end: 0,
            r_completed: 0,
            request_time,
        };

        req
    }

    pub fn set_id(&mut self, id: usize) {
        self.id = id;
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn set_flow_id(&mut self, flow_id: usize) {
        self.flow_id = flow_id;
    }

    pub fn get_flow_id(&self) -> usize {
        self.flow_id
    }

    pub fn set_forwarded_time(&mut self, t_forwarded: usize) {
        self.t_forwarded = t_forwarded;
    }

    pub fn get_forwarded_time(&self) -> usize {
        self.t_forwarded
    }

    pub fn set_arrival_time(&mut self, t_arrival: usize) {
        self.t_arrival = t_arrival;
    }

    pub fn get_arrival_time(&self) -> usize {
        self.t_arrival
    }

    pub fn set_departure_time(&mut self, t_departure: usize) {
        self.t_departure = t_departure;
    }
    
    pub fn get_stack_time(&self) -> usize {
        self.stack_time
    }

    pub fn get_request_time(&self) -> usize {
        self.request_time
    }

    pub fn is_p_completed(&self) -> bool {
        self.is_p_completed
    }

    pub fn is_r_completed(&self) -> bool {
        self.is_r_completed
    }

    pub fn set_p_start(&mut self, t_cur: usize) {
        self.t_p_start = t_cur;
    }

    pub fn set_r_start(&mut self, t_cur: usize) {
        self.t_r_start = t_cur;
    }

    pub fn set_p_end(&mut self, t_cur: usize) {
        self.t_p_end = t_cur;
    }

    pub fn set_r_end(&mut self, t_cur: usize) {
        self.t_r_end = t_cur;
    }

    pub fn set_p_dropped(&mut self) {
        self.is_p_dropped = true;
    }

    pub fn set_r_dropped(&mut self) {
        self.is_r_dropped = true;
    }

    pub fn p_schedule(&mut self) -> bool {
        self.p_completed += 1;
        if self.p_completed == self.stack_time {
            self.is_p_completed = true;
        }

        self.is_p_completed
    }

    pub fn r_schedule(&mut self) -> bool {
        self.r_completed += 1;
        if self.r_completed == self.request_time {
            self.is_r_completed = true;
        }

        self.is_r_completed
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Request #{:?})", self.id)
    }
}