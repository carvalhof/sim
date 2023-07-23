// Created by Fabricio Carvalho (fabricio.carvalho@ufmt.br)

use std::collections::VecDeque;

// Imports
use crate::{
    Request,
    CoreState,
    CoreAction,
};

// Quick Explanation of this

// Structure
pub struct Core {
    core_id: usize,
    is_idle: bool,
    action: CoreAction,
    current_request: Option<Request>,
    local_queue: VecDeque<Request>,
    ready_queue: VecDeque<Request>,
}

impl Core {
    pub fn new(
        core_id: usize,
        action: CoreAction,
        queue_size: usize,
    ) -> Core {
        let local_queue: VecDeque<Request> = VecDeque::<Request>::with_capacity(queue_size);
        let ready_queue: VecDeque<Request> = VecDeque::<Request>::with_capacity(queue_size);

        let core: Core = Core {
            core_id,
            is_idle: true,
            action,
            current_request: None,
            local_queue,
            ready_queue,
        };

        core
    }

    pub fn is_idle(&self) -> bool {
        self.is_idle
    }

    pub fn schedule(&mut self, t_cur: usize, locks: Option<&mut Vec<usize>>) -> CoreState {
        match self.action {
            CoreAction::Forward => {
                // Layout 1
                // In this case, we need only receive the packet and forward to an idle core
                match &mut self.current_request {
                    Some(req) => {
                        if req.f_schedule() {
                            let mut request: Request = self.current_request.take().unwrap();
                            self.current_request = None;
                            request.set_f_end(t_cur);
                            self.is_idle = true;
                            CoreState::Finished(request)
                        } else {
                            self.is_idle = false;
                            CoreState::Running
                        }
                    },
                    None => {
                        if let Some(mut req) = self.local_queue.pop_front() {
                            req.set_f_start(t_cur);
                            req.f_schedule(); // We assume that application time bigger than 1, that why we do not check if application completed
                            self.current_request = Some(req);
                            self.is_idle = false;
                            CoreState::Running
                        } else {
                            self.is_idle = true;
                            CoreState::Idle
                        }
                    }
                }

                // self.is_idle = true;
                // if let Some(mut req) = self.local_queue.pop_front() {
                //     req.set_forwarded_time(t_cur);
                //     CoreState::Finished(req)
                // } else {
                //     CoreState::Idle
                // }
            },
            CoreAction::Application => {
                // Layouts 3 and 4
                // In this case, we need to process only the application
                match &mut self.current_request {
                    Some(req) => {
                        if req.r_schedule() {
                            // If 'req' completed the application, we can finalize it (it will be forward to another core)
                            let mut request: Request = self.current_request.take().unwrap();
                            self.current_request = None;
                            request.set_r_end(t_cur);
                            request.set_departure_time(t_cur + 1);
                            self.is_idle = true;
                            CoreState::Finished(request)
                        } else {
                            self.is_idle = false;
                            CoreState::Running
                        }
                    },
                    None => {
                        if let Some(mut req) = self.local_queue.pop_front() {
                            req.set_r_start(t_cur);
                            req.r_schedule(); // We assume that application time bigger than 1, that why we do not check if application completed
                            self.current_request = Some(req);
                            self.is_idle = false;
                            CoreState::Running
                        } else {
                            self.is_idle = true;
                            CoreState::Idle
                        }
                    }
                }
            },
            CoreAction::NetworkStack => {
                // Layouts 3 and 4
                // In this case, we need to process only the network stack
                match &mut self.current_request {
                    Some(req) => {
                        if req.p_schedule() {
                            // If 'req' completed the network stack, we can finalize it (it will be forward to another core)
                            let mut request: Request = self.current_request.take().unwrap();
                            self.current_request = None;
                            request.set_p_end(t_cur);
                            self.is_idle = true;
                            CoreState::Finished(request)
                        } else {
                            self.is_idle = false;
                            CoreState::Running
                        }
                    },
                    None => {
                        if let Some(mut req) = self.local_queue.pop_front() {
                            req.set_p_start(t_cur);
                            req.p_schedule(); //We assume that network stack time bigger than 1, that why we do not check if network stack completed
                            self.current_request = Some(req);
                            self.is_idle = false;
                            CoreState::Running
                        } else {
                            self.is_idle = true;
                            CoreState::Idle
                        }
                    }
                }
            },
            CoreAction::NetworkStackAndApplication => {
                // Layout 2
                // In this case, we need to process the network stack time and service time separately
                match &mut self.current_request {
                    Some(req) => {
                        if req.is_p_completed() {
                            // If 'req' completed network stack processing, we can go to the application processing
                            if req.r_schedule() {
                                // If 'req' completed both network stack and application processing, we can finalize it
                                let mut request: Request = self.current_request.take().unwrap();
                                self.current_request = None;
                                
                                request.set_r_end(t_cur);
                                request.set_departure_time(t_cur + 1);
                                self.is_idle = true;
                                CoreState::Finished(request)
                            } else {
                                // It means that we still need process the request through application processing
                                self.is_idle = false;
                                CoreState::Running
                            }
                        } else {
                            // It means that we still need process the request through network stack processing
                            if req.p_schedule() {
                                // If 'req' completed right now, we set the application request in the next round
                                req.set_p_end(t_cur);
                                req.set_r_start(t_cur + 1);
                            }
                            self.is_idle = false;
                            CoreState::Running
                        }
                    },
                    None => {
                        if let Some(mut req) = self.local_queue.pop_front() {
                            req.set_p_start(t_cur);
                            req.p_schedule(); //We assume that network stack time bigger than 1, that why we do not check if network stack completed
                            self.current_request = Some(req);
                            self.is_idle = false;
                            CoreState::Running
                        } else {
                            self.is_idle = true;
                            CoreState::Idle
                        }
                    }
                }
            },
            CoreAction::NetworkStackAndApplicationLock => {
                // Layout 1
                // In this case, we need to process the network stack time and service time separately
                match &mut self.current_request {
                    Some(req) => {
                        let spinlocks: &mut Vec<usize> = locks.unwrap();
                        if spinlocks[req.get_flow_id()] == usize::MAX {
                            // This means that will be the first time that this core will process this request
                            spinlocks[req.get_flow_id()] = self.core_id;
                            req.set_p_start(t_cur);
                            req.p_schedule(); //We assume that network stack time bigger than 1, that why we do not check if network stack completed
                            self.is_idle = false;
                            CoreState::Running
                        } else if spinlocks[req.get_flow_id()] == self.core_id {
                            if req.is_p_completed() {
                                // If 'req' completed network stack processing, we can go to the application processing
                                if req.r_schedule() {
                                    // If 'req' completed both network stack and application processing, we can finalize it
                                    let mut request: Request = self.current_request.take().unwrap();
                                    self.current_request = None;
                                    request.set_r_end(t_cur);
                                    request.set_departure_time(t_cur + 1);
                                    self.is_idle = true;
                                    spinlocks[request.get_flow_id()] = usize::MAX;
                                    CoreState::Finished(request)
                                } else {
                                    // It means that we still need process the request through application processing
                                    self.is_idle = false;
                                    CoreState::Running
                                }
                            } else {
                                // It means that we still need process the request through network stack processing
                                if req.p_schedule() {
                                    // If 'req' completed right now, we set the application request in the next round
                                    req.set_p_end(t_cur);
                                    req.set_r_start(t_cur + 1);
                                }
                                self.is_idle = false;
                                CoreState::Running
                            }
                        } else {
                            // Another worker is holding the lock for this request
                            self.is_idle = false;
                            CoreState::Running
                        }
                    },
                    None => {
                        if let Some(mut req) = self.local_queue.pop_front() {
                            let spinlocks: &mut Vec<usize> = locks.unwrap();
                            if spinlocks[req.get_flow_id()] == usize::MAX {
                                // This indicates that no worker is processing this flow
                                spinlocks[req.get_flow_id()] = self.core_id;
                                req.set_p_start(t_cur);
                                req.p_schedule(); //We assume that network stack time bigger than 1, that why we do not check if network stack completed
                            }
                            self.current_request = Some(req);
                            self.is_idle = false;
                            CoreState::Running
                        } else {
                            self.is_idle = true;
                            CoreState::Idle
                        }
                    }
                }
            }
        }
    }

    pub fn get_id(&self) -> usize {
        self.core_id
    }

    pub fn get_action(&self) -> &CoreAction {
        &self.action
    }

    pub fn try_enqueue(&mut self, req: Request) -> Result<(), Request> {
        if self.local_queue.len() < self.local_queue.capacity() {
            self.local_queue.push_back(req);
            return Ok(())
        }
        Err(req)
    }

    pub fn try_enqueue_ready_queue(&mut self, req: Request) -> Result<(), Request> {
        if self.ready_queue.len() < self.ready_queue.capacity() {
            self.ready_queue.push_back(req);
            return Ok(())
        }
        Err(req)
    }

    pub fn pop_ready_queue(&mut self) -> Request {
        self.ready_queue.pop_front().unwrap()
    }
}