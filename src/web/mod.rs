mod data;

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use warp::Filter;

use crate::model::Packet;

pub struct PingSource {
    pub target: String,
    pub history: Arc<Mutex<VecDeque<Packet>>>,
    pub mtu_history: Arc<Mutex<VecDeque<(u128, u32)>>>,
}

impl Clone for PingSource {
    fn clone(&self) -> Self {
        PingSource {
            target: self.target.clone(),
            history: Arc::clone(&self.history),
            mtu_history: Arc::clone(&self.mtu_history),
        }
    }
}

pub async fn serve(
    port: u16,
    loopback_history: Arc<Mutex<VecDeque<Packet>>>,
    loopback_mtu: Arc<Mutex<VecDeque<(u128, u32)>>>,
    ping_sources: Vec<PingSource>,
) {
    let report_route = warp::path::end().and_then(move || {
        let loopback_history = Arc::clone(&loopback_history);
        let loopback_mtu = Arc::clone(&loopback_mtu);
        let ping_sources = ping_sources.clone();

        async move {
            // ── loopback data ──────────────────────────────────────────────
            let lb_lat = data::latency_rows(Arc::clone(&loopback_history)).await;
            let lb_qual = data::quality_rows(Arc::clone(&loopback_history)).await;
            let lb_mtu = data::mtu_rows(Arc::clone(&loopback_mtu)).await;

            // ── ping data ──────────────────────────────────────────────────
            let mut ping_js_vars = String::new();   // variable declarations + draw calls
            let mut ping_divs = String::new();       // HTML divs

            for (i, src) in ping_sources.iter().enumerate() {
                let lat = data::latency_rows(Arc::clone(&src.history)).await;
                let qual = data::quality_rows(Arc::clone(&src.history)).await;
                let mtu = data::mtu_rows(Arc::clone(&src.mtu_history)).await;
                let target = &src.target;

                ping_js_vars.push_str(&format!(r#"
        // ── {target} ──
        var plat{i} = new google.visualization.DataTable();
        plat{i}.addColumn('date','Date');
        plat{i}.addColumn('number','min'); plat{i}.addColumn('number','max'); plat{i}.addColumn('number','median');
        plat{i}.addRows([{lat}]);
        new google.visualization.AnnotationChart(document.getElementById('plat{i}')).draw(plat{i}, CHART_OPTS_SM);

        var pqual{i} = new google.visualization.DataTable();
        pqual{i}.addColumn('date','Date');
        pqual{i}.addColumn('number','Loss %'); pqual{i}.addColumn('number','Reorders'); pqual{i}.addColumn('number','Duplicates');
        pqual{i}.addRows([{qual}]);
        new google.visualization.AnnotationChart(document.getElementById('pqual{i}')).draw(pqual{i}, CHART_OPTS_XS);

        var pmtu{i} = new google.visualization.DataTable();
        pmtu{i}.addColumn('date','Date');
        pmtu{i}.addColumn('number','MTU bytes');
        pmtu{i}.addRows([{mtu}]);
        new google.visualization.AnnotationChart(document.getElementById('pmtu{i}')).draw(pmtu{i}, CHART_OPTS_XS);
"#));

                ping_divs.push_str(&format!(r#"
    <h3 style='font-family:sans-serif;margin:24px 0 4px'>ICMP Ping to {target} — latency (µs)</h3>
    <div id='plat{i}' style='width:900px;height:300px'></div>
    <h4 style='font-family:sans-serif;margin:16px 0 4px'>ICMP Ping to {target} — quality</h4>
    <div id='pqual{i}' style='width:900px;height:200px'></div>
    <h4 style='font-family:sans-serif;margin:16px 0 4px'>ICMP Ping to {target} — MTU (bytes)</h4>
    <div id='pmtu{i}' style='width:900px;height:200px'></div>
"#));
            }

            let html = format!(r#"<!DOCTYPE html>
<html>
<head>
  <script src='https://www.gstatic.com/charts/loader.js'></script>
  <script>
    google.charts.load('current', {{'packages':['annotationchart']}});
    google.charts.setOnLoadCallback(function() {{
      var ZOOM = {{zoomStartTime: new Date(new Date().getTime() - 24*60*60*1000)}};
      var BASE = {{displayAnnotations:true, scaleType:'allfixed', legendPosition:'newRow', thickness:2}};
      function merge(extra) {{ return Object.assign({{}}, BASE, ZOOM, extra); }}
      var CHART_OPTS_LG = merge({{}}); // 500 px
      var CHART_OPTS_SM = merge({{}});  // 300 px
      var CHART_OPTS_XS = merge({{}});  // 200 px

      // ── UDP Loopback latency ──────────────────────────────────────────
      var lbLat = new google.visualization.DataTable();
      lbLat.addColumn('date','Date');
      lbLat.addColumn('number','min'); lbLat.addColumn('number','max'); lbLat.addColumn('number','median');
      lbLat.addRows([{lb_lat}]);
      new google.visualization.AnnotationChart(document.getElementById('lb_lat')).draw(lbLat, CHART_OPTS_LG);

      // ── UDP Loopback quality ──────────────────────────────────────────
      var lbQual = new google.visualization.DataTable();
      lbQual.addColumn('date','Date');
      lbQual.addColumn('number','Loss %'); lbQual.addColumn('number','Reorders'); lbQual.addColumn('number','Duplicates');
      lbQual.addRows([{lb_qual}]);
      new google.visualization.AnnotationChart(document.getElementById('lb_qual')).draw(lbQual, CHART_OPTS_SM);

      // ── UDP Loopback MTU ──────────────────────────────────────────────
      var lbMtu = new google.visualization.DataTable();
      lbMtu.addColumn('date','Date');
      lbMtu.addColumn('number','MTU bytes');
      lbMtu.addRows([{lb_mtu}]);
      new google.visualization.AnnotationChart(document.getElementById('lb_mtu')).draw(lbMtu, CHART_OPTS_XS);

      {ping_js_vars}
    }});
  </script>
</head>
<body>
  <h2 style='font-family:sans-serif;margin:16px 0 4px'>UDP Loopback — latency (µs)</h2>
  <div id='lb_lat' style='width:900px;height:500px'></div>
  <h3 style='font-family:sans-serif;margin:24px 0 4px'>UDP Loopback — quality</h3>
  <div id='lb_qual' style='width:900px;height:300px'></div>
  <h3 style='font-family:sans-serif;margin:24px 0 4px'>UDP Loopback — MTU (bytes)</h3>
  <div id='lb_mtu' style='width:900px;height:200px'></div>
  {ping_divs}
</body>
</html>"#);

            Ok::<_, warp::Rejection>(warp::reply::html(html))
        }
    });

    println!("Report also available via HTTP port {port}");
    warp::serve(report_route).run(([0, 0, 0, 0], port)).await;
}
