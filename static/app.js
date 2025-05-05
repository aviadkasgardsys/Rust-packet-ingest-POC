// 1) Chart options with markers enabled
const opts = {
  title: "Live Packets Per Second",
  width: window.innerWidth,
  height: window.innerHeight,
  scales: {
    x: { time: false }, // using relative seconds on X
    y: { auto: true },
  },
  axes: [{ label: "s" }, { label: "length" }],
  series: [
    {}, // x-axis (required placeholder)
    {
      label: "value",
      width: 2, // line thickness
      stroke: "blue", // line color
      points: {
        show: true, // enable dots
        size: 4, // dot radius
        stroke: "white", // dot outline
        fill: "blue", // dot fill
      },
    },
  ],
};

// 2) State holders
let chart, data, t0;
let buffer = [];
const SMOOTH_WINDOW = 5;

// Simple moving‐average smoother
function smooth(val) {
  buffer.push(val);
  if (buffer.length > SMOOTH_WINDOW) buffer.shift();
  return buffer.reduce((a, b) => a + b, 0) / buffer.length;
}

// 3) Initialize chart on first incoming timestamp
function initChart(tsMs) {
  t0 = tsMs;
  // seed with two points (0→1s at pps=0)
  data = [
    [0, 1], // seconds
    [0, 0], // pps
  ];
  chart = new uPlot(opts, data, document.getElementById("chart"));
}

// 4) Add and throttle updates
let pending = false;
function addPoint(tsMs, ppsRaw) {
  const sec = (tsMs - t0) / 1000; // relative seconds
  const pps = smooth(ppsRaw); // smoothed value

  data[0].push(sec);
  data[1].push(pps);

  // keep at most 60 points
  if (data[0].length > 60) {
    data[0].shift();
    data[1].shift();
  }

  // throttle to one redraw per animation frame
  if (!pending) {
    pending = true;
    requestAnimationFrame(() => {
      chart.setData(data);
      pending = false;
    });
  }
}

// 5) SSE connection and handler
function connect() {
  const src = new EventSource(`http://${location.hostname}:3031/signal`);

  src.onmessage = (e) => {
    try {
      const msg = JSON.parse(e.data);
      if (msg.type === "Data") {
        // server timestamp is in nanoseconds
        const tsMs = msg.payload.timestamp / 1e6;

        if (!chart) {
          initChart(tsMs);
        }
        addPoint(tsMs, msg.payload.value);
      }
    } catch (err) {
      console.error("SSE parse error:", err);
    }
  };

  src.onerror = () => {
    console.warn("Connection lost; retrying in 1s");
    src.close();
    setTimeout(connect, 1000);
  };
}

// 6) Start streaming
connect();
