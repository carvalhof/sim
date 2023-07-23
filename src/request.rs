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
    // Forwarder
    is_f_dropped: bool,
    is_f_completed: bool,
    // Packet
    is_p_dropped: bool,
    is_p_completed: bool,
    // Request
    is_r_dropped: bool,
    is_r_completed: bool,
    // Packet infomation
    flow_id: usize,
    // Forward information
    t_f_start: usize,
    t_f_end: usize,
    f_completed: usize,
    forward_time: usize,
    // Packet information
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
        forward_time: usize,
        stack_time: usize,
        request_time: usize,
    ) -> Request {
        let req = Request {
            id,
            t_arrival,
            t_departure: 0,
            is_f_dropped: false,
            is_f_completed: false,
            is_p_dropped: false,
            is_p_completed: false,
            is_r_dropped: false,
            is_r_completed: false,
            flow_id,
            t_f_start: 0,
            t_f_end: 0,
            f_completed: 0,
            forward_time,
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

    pub fn get_start(&self) -> usize {
        self.t_p_start
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
    
    pub fn get_departure_time(&self) -> usize {
        self.t_departure
    }

    pub fn get_stack_time(&self) -> usize {
        self.stack_time
    }

    pub fn get_request_time(&self) -> usize {
        self.request_time
    }

    pub fn is_f_completed(&self) -> bool {
        self.is_f_completed
    }

    pub fn is_p_completed(&self) -> bool {
        self.is_p_completed
    }

    pub fn is_r_completed(&self) -> bool {
        self.is_r_completed
    }

    pub fn set_f_start(&mut self, t_cur: usize) {
        self.t_f_start = t_cur;
    }

    pub fn set_p_start(&mut self, t_cur: usize) {
        self.t_p_start = t_cur;
    }

    pub fn set_r_start(&mut self, t_cur: usize) {
        self.t_r_start = t_cur;
    }

    pub fn set_f_end(&mut self, t_cur: usize) {
        self.t_f_end = t_cur;
    }

    pub fn set_p_end(&mut self, t_cur: usize) {
        self.t_p_end = t_cur;
    }

    pub fn set_r_end(&mut self, t_cur: usize) {
        self.t_r_end = t_cur;
    }

    pub fn set_f_dropped(&mut self) {
        self.is_f_dropped = true;
    }

    pub fn set_p_dropped(&mut self) {
        self.is_p_dropped = true;
    }

    pub fn set_r_dropped(&mut self) {
        self.is_r_dropped = true;
    }

    pub fn f_schedule(&mut self) -> bool {
        self.f_completed += 1;
        if self.f_completed == self.forward_time {
            self.is_f_completed = true;
        }

        self.is_f_completed
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