// Created by Fabricio Carvalho (fabricio.carvalho@ufmt.br)

use std::collections::VecDeque;

// Imports
use crate::{
    Phase,   
    Request, request,
};

// Quick Explanation of this

// Structure
pub struct Core {
    core_id: usize,
    phase: Phase,
    current_request: Option<Request>,
    local_queue: VecDeque<Request>,
}

impl Core {
    pub fn new(
        core_id: usize,
        phase: Phase,
        queue_size: usize,
    ) -> Core {
        let local_queue: VecDeque<Request> = VecDeque::<Request>::with_capacity(queue_size);

        let core = Core {
            core_id,
            phase,
            current_request: None,
            local_queue,
        };

        core
    }

    pub fn schedule(&mut self, t_cur: usize) {
        match self.phase {
            Phase::Phase1 => todo!(),
            Phase::Phase2 => todo!(),
            Phase::Phase3 => todo!(),
            Phase::Phase4 => {
                match self.current_request.as_mut() {
                    Some(req) => {
                        req.schedule();
                        if req.is_completed() {
                            self.current_request = None;
                        }
                    },
                    None => {
                        match self.local_queue.pop_front() {
                            Some(mut req) => {
                                req.set_start(t_cur);
                                req.schedule();
                                self.current_request = Some(req);
                            },
                            None => {
                                //self.nr_idle += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn try_enqueue(&mut self, req: Request) -> Result<(), Request> {
        if self.local_queue.len() < self.local_queue.capacity() {
            self.local_queue.push_back(req);
            return Ok(())
        }
        Err(req)
    }
}