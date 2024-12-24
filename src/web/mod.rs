use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use warp::Filter;

pub async fn serve(port: u16, history: Arc<Mutex<VecDeque<u64>>>, interval_millis: u64) {
    let report_route = warp::path::end()
        .and_then(move || {
            async move {
                let mut html = String::from("<html>
      <head>
        <script type='text/javascript' src='https://www.gstatic.com/charts/loader.js'></script>
        <script type='text/javascript'>
          google.charts.load('current', {'packages':['annotationchart']});
          google.charts.setOnLoadCallback(drawChart);
          function drawChart() {");

                let rows1 = String::new();

                // Prepare the column definitions
                let mut columns1 = String::from("data1.addColumn('date', 'Date');\n");
                    columns1.push_str(&format!(
                        "data1.addColumn('number', '1');\n",
                    ));
                html = format!(r#"{html}
                 var data1 = new google.visualization.DataTable();
            {columns1}
            data1.addRows([
                {rows1}
            ]);

            var chart1 = new google.visualization.AnnotationChart(document.getElementById('chart_div1'));
            chart1.draw(data1, {{
              displayAnnotations: true,
              scaleType: 'allfixed',
              legendPosition: 'newRow',
              thickness: 2,
              zoomStartTime: new Date(new Date().getTime() - 24*60*60*1000)  // Start from 24 hours ago
            }});
          }}
        </script>
      </head>

      <body>
        <div id='chart_div1' style='width: 900px; height: 500px;'></div>
      </body>
    </html>
    "#
                );
                Ok::<_, warp::Rejection>(warp::reply::html(html))
            }
        });

    println!("Report also available via HTTP port {port}");
    warp::serve(report_route).run(([0, 0, 0, 0], port)).await;
}
