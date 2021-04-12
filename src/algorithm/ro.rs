use crate::{MAX_K, cnc::Toolbox, network::Path};
use crate::component::Solution;
use crate::component::FlowTable;
use crate::network::Network;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaChaRng;
use std::time::Instant;
use super::base::yens::Yens;
use super::Algorithm;


const ALPHA_PORTION: f64 = 0.5;


pub struct RO {
    yens: Yens,
    seed: u64,
}


impl Algorithm for RO {
    fn candidates(&self, src: usize, dst: usize) -> &Vec<Path> {
        self.yens.k_shortest_paths(src, dst)
    }
    fn prepare(&mut self, _solution: &mut Solution, _flowtable: &FlowTable) {}
    /// 在所有 TT 都被排定的狀況下去執行 GRASP 優化
    fn configure(&mut self, solution: &mut Solution, deadline: Instant, toolbox: Toolbox) {
        let flowtable = solution.flowtable();
        // self.grasp(solution, deadline);
        let mut rng = ChaChaRng::seed_from_u64(self.seed);
        let mut iter_times = 0;
        let mut min_cost = toolbox.evaluate_cost(solution);
        while Instant::now() < deadline {
            iter_times += 1;
            // PHASE 1
            let mut current = solution.clone();
            for &avb in flowtable.avbs() {
                let (src, dst) = flowtable.ends(avb);
                let candidate_cnt = self.get_candidate_count(src, dst);
                let alpha = (candidate_cnt as f64 * ALPHA_PORTION) as usize;
                let set = gen_n_distinct_outof_k(alpha, candidate_cnt, &mut rng);
                let new_route = self.find_min_cost_route(solution, avb, Some(set), &flowtable, &toolbox);
                current.select(avb, new_route);
            }
            // PHASE 2
            let cost = toolbox.evaluate_cost(&mut current);
            if cost.0 < min_cost.0 {
                min_cost = cost;
                // #[cfg(debug_assertions)]
                // println!("found min_cost = {:?} at first glance!", cost);
            }

            #[cfg(debug_assertions)]
            println!("start iteration #{}", iter_times);
            // self.hill_climbing(solution, &mut rng, &deadline, &mut min_cost, current);

            let mut iter_times_inner = 0;
            while Instant::now() < deadline {
                if min_cost.1 {
                    break; // 找到可行解，返回
                }

                let rand = rng
                    .gen_range(0..flowtable.len());
                let target_id = rand.into();
                if flowtable.avb_spec(target_id).is_none() {
                    continue;
                }

                let new_route = self.find_min_cost_route(solution, target_id, None, &flowtable, &toolbox);
                let old_route = solution
                    .selection(target_id).current()
                    .unwrap();

                if old_route == new_route {
                    continue;
                }

                // 實際更新下去，並計算成本
                current.select(target_id, new_route);
                let cost = toolbox.evaluate_cost(&mut current);

                if cost.0 < min_cost.0 {
                    *solution = current.clone();
                    min_cost = cost.clone();
                    iter_times_inner = 0;

                    // #[cfg(debug_assertions)]
                    // println!("found min_cost = {:?}", cost);
                } else {
                    // 恢復上一動
                    current.select(target_id, old_route);
                    iter_times_inner += 1;
                    if iter_times_inner == flowtable.len() {
                        //  NOTE: 迭代次數上限與資料流數量掛勾
                        break;
                    }
                }
            }

            if min_cost.1 {
                // 找到可行解，且為快速終止模式
                break;
            }
            println!("{:?}", iter_times);
            println!("{:?}", min_cost);
        }
    }
}

impl RO {
    pub fn new(network: &Network, seed: u64) -> Self {
        let yens = Yens::new(&network, MAX_K);
        RO { yens, seed }
    }
    /// 若有給定候選路徑的子集合，就從中選。若無，則遍歷所有候選路徑
    fn find_min_cost_route(&self, solution: &Solution, id: usize, set: Option<Vec<usize>>, flowtable: &FlowTable, toolbox: &Toolbox) -> usize {
        let (src, dst) = flowtable.ends(id);
        let (mut min_cost, mut best_k) = (std::f64::MAX, 0);
        let mut closure = |kth: usize| {
            let wcd = toolbox.evaluate_wcd(id, kth, solution) as f64;
            if wcd < min_cost {
                min_cost = wcd;
                best_k = kth;
            }
        };
        if let Some(vec) = set {
            for k in vec.into_iter() {
                closure(k);
            }
        } else {
            for k in 0..self.get_candidate_count(src, dst) {
                closure(k);
            }
        }
        best_k
    }
    fn get_candidate_count(&self, src: usize, dst: usize) -> usize {
        self.yens.count_shortest_paths(src, dst)
    }
}

fn gen_n_distinct_outof_k(n: usize, k: usize, rng: &mut ChaChaRng) -> Vec<usize> {
    let mut vec = Vec::with_capacity(n);
    for i in 0..k {
        let rand = rng.gen();
        let random: usize = rand;
        vec.push((random, i));
    }
    vec.sort();
    vec.into_iter().map(|(_, i)| i).take(n).collect()
}
