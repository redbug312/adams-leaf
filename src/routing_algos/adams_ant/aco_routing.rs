use super::AdamsAnt;
use crate::network_wrapper::RoutingCost;
use crate::recorder::flow_table::prelude::*;
use crate::util::aco::{ACOJudgeResult, ACO};
use crate::{FAST_STOP, MAX_K};
use std::time::Instant;

const TSN_MEMORY: f64 = 3.0; // 計算能見度時，TSN 對舊路徑的偏好程度
const AVB_MEMORY: f64 = 3.0; // 計算能見度時，AVB 對舊路徑的偏好程度

pub fn do_aco(algo: &mut AdamsAnt, time_limit: u128) {
    let time = Instant::now();
    let aco = &mut algo.aco as *mut ACO;

    let vis = compute_visibility(algo);

    let mut best_dist = dist_computing(&algo.wrapper.compute_all_cost());
    let new_state = unsafe {
        (*aco).do_aco(time_limit - time.elapsed().as_micros(), &vis, |state| {
            let (cost, dist) = compute_aco_dist(algo, state, &mut best_dist);
            if cost.avb_fail_cnt == 0 && FAST_STOP {
                // 找到可行解，且為快速終止模式
                ACOJudgeResult::Stop(dist)
            } else {
                ACOJudgeResult::KeepOn(dist)
            }
        })
    };
}

fn compute_visibility(algo: &AdamsAnt) -> Vec<[f64; MAX_K]> {
    // TODO 好好設計能見度函式！
    // 目前：路徑長的倒數
    let len = algo.aco.get_state_len();
    let mut vis = vec![[0.0; MAX_K]; len];
    for (flow, _) in algo.wrapper.get_flow_table().iter_avb() {
        let id = flow.id;
        for i in 0..algo.get_candidate_count(flow) {
            vis[id.0][i] = 1.0 / algo.wrapper.compute_avb_wcd(flow, Some(&i)) as f64;
        }
        if let Some(&route_k) = algo.wrapper.get_old_route(id) {
            // 是舊資料流，調高本來路徑的能見度
            vis[id.0][route_k] *= AVB_MEMORY;
        }
    }
    for (flow, _) in algo.wrapper.get_flow_table().iter_tsn() {
        let id = flow.id;
        for i in 0..algo.get_candidate_count(flow) {
            let yens = algo.yens_algo.borrow();
            let route = yens.get_kth_route(flow.src, flow.dst, i);
            vis[id.0][i] = 1.0 / route.len() as f64;
        }

        if let Some(&route_k) = algo.wrapper.get_old_route(id) {
            // 是舊資料流，調高本來路徑的能見度
            vis[id.0][route_k] *= TSN_MEMORY;
        }
    }
    vis
}

/// 本函式不只會計算距離，如果看見最佳解，還會把該解的 FlowTable 和 GCL 記錄下來
unsafe fn compute_aco_dist(
    algo: &mut AdamsAnt,
    state: &Vec<usize>,
    best_dist: &mut f64,
) -> (RoutingCost, f64) {
    let mut cur_wrapper = algo.wrapper.clone();
    let mut diff = cur_wrapper.get_flow_table().clone_as_diff();

    for (id, &route_k) in state.iter().enumerate() {
        // NOTE: 若發現和舊的資料一樣，這個 update_info 函式會自動把它忽略掉
        diff.update_info(id.into(), route_k);
    }

    cur_wrapper.update_tsn(&diff);
    cur_wrapper.update_avb(&diff);
    let cost = cur_wrapper.compute_all_cost();
    let dist = dist_computing(&cost);

    if dist < *best_dist {
        *best_dist = dist;
        // 記錄 FlowTable 及 GCL
        algo.wrapper = cur_wrapper;
    }

    (cost, dist)
}

fn dist_computing(cost: &RoutingCost) -> f64 {
    let base: f64 = 10.0;
    base.powf(cost.compute() - 1.0)
}
