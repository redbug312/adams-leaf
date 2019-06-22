use crate::MAX_K;
use crate::util::aco::ACO;
use super::{FlowTable, AdamsAnt, compute_avb_cost, compute_all_avb_cost};
use super::super::CostCalculator;

pub fn do_aco(algo: &mut AdamsAnt, time_limit: u128, changed: FlowTable<usize>) {
    let aco = &mut algo.aco as *mut ACO;
    algo.g.forget_all_flows();
    algo.flow_table.foreach(true, |flow, &route_k| unsafe {
        algo.save_flowid_on_edge(true, *flow.id(), route_k);
    });

    let mut calc = CostCalculator::new(algo.aco.get_state_len(), 0.0);
    algo.flow_table.foreach(true, |flow, &route_k| {
        let cost = algo.compute_avb_cost(flow, None);
        calc.set_cost(*flow.id(), cost);
    });

    let mut table = algo.flow_table.clone();
    let mut gcl = algo.gcl.clone();
    let vis = compute_visibility(algo, changed);
    let new_state = unsafe {
        (*aco).do_aco(time_limit, &vis, |state| {
            for (id, &route_k) in state.iter().enumerate() {
                if table.check_flow_exist(id) {
                    let old_route_k = *table.get_info(id);
                    if old_route_k != route_k {
                        // 資料流存在，且在蟻群算法途中發生改變
                        if table.get_flow(id).is_avb() {
                            algo.save_flowid_on_edge(false, id, old_route_k);
                            algo.save_flowid_on_edge(true, id, route_k);
                            // TODO 透過只計算受影響的資料流來加速
                            table.update_info(id, route_k);
                        } else {
                            // TODO 重排 TT
                        }
                    }

                }
            }
            let mut cost = compute_all_avb_cost(algo, &table, &gcl);
            println!("{:?} {}", state, cost * algo.avb_count as f64);
            cost /= algo.avb_count as f64;
            cost * cost
        }, calc.get_total_cost())
    };
    if let Some(new_state) = new_state {
        for (id, &route_k) in new_state.iter().enumerate() {
            algo.flow_table.update_info(id, route_k);
        }
    }
}

fn compute_visibility(algo: &AdamsAnt, changed: FlowTable<usize>) -> Vec<[f64; MAX_K]> {
    // TODO 好好設計能見度函式！
    // 目前：AVB 選中本來路徑的機率是改路徑機率的10倍
    //      TT 釘死最短路徑
    let len = algo.aco.get_state_len();
    let mut vis = vec![[0.0; MAX_K]; len];
    algo.flow_table.foreach(true, |flow, &route_k| {
        let id = *flow.id();
        for i in 0..algo.get_candidate_count(flow) {
            if changed.check_flow_exist(id) { // 是新資料流，賦與所有路徑平均的能見度
                vis[id][i] = 10.0;
            } else { // 是舊資料流，壓低其它路徑的能見度
                vis[id][i] = 1.0;
            }
        }
        vis[id][route_k] = 10.0;
    });
    algo.flow_table.foreach(false, |flow, &route_k| {
        let id = *flow.id();
        vis[id][0] = 10.0;
    });
    vis
}