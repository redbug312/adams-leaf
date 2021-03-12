use super::RoutingAlgo;
use crate::MAX_K;
use crate::utils::stream::{TSN, AVB};
use crate::network::Network;
use crate::component::{NetworkWrapper, RoutingCost};
use super::base::yens::YensAlgo;
use std::{cell::RefCell, rc::Rc, time::Instant};

pub struct SPF {
    compute_time: u128,
    wrapper: NetworkWrapper,
}

impl SPF {
    pub fn new(g: Network) -> Self {
        let yens_algo = Rc::new(RefCell::new(YensAlgo::default()));
        let tmp_yens = yens_algo.clone();
        tmp_yens.borrow_mut().compute(&g, MAX_K);
        let wrapper = NetworkWrapper::new(g, move |src, dst, _| {
            tmp_yens.borrow().kth_shortest_path(src, dst, 0).unwrap()
                as *const Vec<usize>
        });
        SPF {
            compute_time: 0,
            wrapper,
        }
    }
}

impl RoutingAlgo for SPF {
    fn get_last_compute_time(&self) -> u128 {
        self.compute_time
    }
    fn add_flows(&mut self, tsns: Vec<TSN>, avbs: Vec<AVB>) {
        let init_time = Instant::now();
        for flow in tsns.into_iter() {
            self.wrapper.insert(vec![flow], vec![], 0);
        }
        for flow in avbs.into_iter() {
            self.wrapper.insert(vec![], vec![flow], 0);
        }
        self.compute_time = init_time.elapsed().as_micros();
    }
    fn get_rerouted_flows(&self) -> &Vec<usize> {
        unimplemented!();
    }
    fn get_route(&self, id: usize) -> &Vec<usize> {
        self.wrapper.get_route(id)
    }
    fn show_results(&self) {
        println!("TT Flows:");
        for &id in self.wrapper.get_flow_table().iter_tsn() {
            let route = self.get_route(id);
            println!("flow id = FlowID({:?}), route = {:?}", id, route);
        }
        println!("AVB Flows:");
        for &id in self.wrapper.get_flow_table().iter_avb() {
            let route = self.get_route(id);
            let cost = self.wrapper.compute_single_avb_cost(id);
            println!(
                "flow id = FlowID({:?}), route = {:?} avb wcd / max latency = {:?}, reroute = {}",
                id, route, cost.avb_wcd, cost.reroute_overhead
            );
        }
        let all_cost = self.wrapper.compute_all_cost();
        println!("the cost structure = {:?}", all_cost,);
        println!("{}", all_cost.compute());
    }
    fn get_cost(&self) -> RoutingCost {
        self.wrapper.compute_all_cost()
    }
}