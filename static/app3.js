// app3.js

async function initLiveGraph() {
  if (!navigator.gpu) {
    console.error("WebGPU not supported");
    return;
  }
  const adapter = await navigator.gpu.requestAdapter();
  const device = await adapter.requestDevice();
  const canvas = document.getElementById("gpu-canvas");
  const tooltip = document.getElementById("tooltip");
  const context = canvas.getContext("webgpu");
  const format = navigator.gpu.getPreferredCanvasFormat();
  let lastMouseX = 0;
  let lastMouseY = 0;
  let startIdx = 0;

  // High-res clock (ns)
  const perfOriginNs = BigInt(Math.floor(performance.timeOrigin * 1e6));
  function nowNs() {
    return perfOriginNs + BigInt(Math.floor(performance.now() * 1e6));
  }

  // Resize canvas
  function resize() {
    const dpr = window.devicePixelRatio || 1;
    canvas.width = canvas.clientWidth * dpr;
    canvas.height = canvas.clientHeight * dpr;
    context.configure({
      device,
      format,
      alphaMode: "opaque",
      width: canvas.width,
      height: canvas.height,
    });
  }
  window.addEventListener("resize", resize);
  resize();

  const wgsl = `

  struct VertexIn {
    @location(0) xy: vec2<f32>,
  };                                
  
  struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0)       color:    vec4<f32>,
  };                                       
  

  @vertex
  fn vs_main(in: VertexIn) -> VertexOut {
    var o: VertexOut;
    o.position = vec4(in.xy, 0.0, 1.0);
    o.color    = vec4(0.0, 0.8, 1.0, 1.0);
    return o;
  }
  
  @fragment
  fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return in.color;
  }
  `;
  const module = device.createShaderModule({ code: wgsl });
  const pipeline = device.createRenderPipeline({
    layout: "auto",
    vertex: {
      module,
      entryPoint: "vs_main",
      buffers: [
        {
          arrayStride: 8,
          attributes: [{ shaderLocation: 0, offset: 0, format: "float32x2" }],
        },
      ],
    },
    fragment: {
      module,
      entryPoint: "fs_main",
      targets: [{ format }],
    },
    primitive: { topology: "line-strip" },
  });
  const axisWGSL = `
  //–– axis inputs ––
  struct AxisIn {
    @location(0) xy: vec2<f32>,
  };   
  
  @vertex
  fn vs_axis(in: AxisIn) -> @builtin(position) vec4<f32> {
    return vec4(in.xy, 0.0, 1.0);
  }
  
  @fragment
  fn fs_axis() -> @location(0) vec4<f32> {
    return vec4(0.5, 0.5, 0.5, 1.0);
  }
  `;

  // Shader that outputs a constant grey
  const axisModule = device.createShaderModule({ code: axisWGSL });
  const axisPipeline = device.createRenderPipeline({
    layout: "auto",
    vertex: {
      module: axisModule,
      entryPoint: "vs_axis",
      buffers: [
        {
          arrayStride: 8,
          attributes: [{ shaderLocation: 0, offset: 0, format: "float32x2" }],
        },
      ],
    },
    fragment: {
      module: axisModule,
      entryPoint: "fs_axis",
      targets: [{ format }],
    },
    primitive: { topology: "line-list" },
  });

  // Upload axes (2 segments)
  const axisVerts = new Float32Array([
    // X axis
    -1, 0, 1, 0,
    // Y axis
    0, -1, 0, 1,
  ]);
  const axisBuffer = device.createBuffer({
    size: axisVerts.byteLength,
    usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
  });
  device.queue.writeBuffer(axisBuffer, 0, axisVerts);

  // Upload grid lines every 10s (6 vertical + 6 horizontal)
  const gridVerts = [];
  for (let i = 1; i < 6; i++) {
    const t = (i * 10) / 60; // 0…1
    const x = 1 - t * 2; // map to NDC
    gridVerts.push(x, -1, x, 1);
    gridVerts.push(-1, x, 1, x);
  }
  const gridBuffer = device.createBuffer({
    size: Float32Array.BYTES_PER_ELEMENT * gridVerts.length,
    usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
  });
  device.queue.writeBuffer(gridBuffer, 0, new Float32Array(gridVerts));

  // — rest of your setup: buffers, workers, etc. —
  const MAX = 64000 * 60;
  const tsBuf = new BigInt64Array(MAX);
  const valBuf = new Uint32Array(MAX);
  const vertArr = new Float32Array(MAX * 2);
  const vbo = device.createBuffer({
    size: vertArr.byteLength,
    usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
  });
  let writeIdx = 0,
    count = 0;

  // Triple‐buffered staging
  const FRAMES_IN_FLIGHT = 3;
  const staging = [];
  for (let i = 0; i < FRAMES_IN_FLIGHT; i++) {
    const buf = device.createBuffer({
      size: MAX * 2 * 4,
      usage: GPUBufferUsage.MAP_WRITE | GPUBufferUsage.COPY_SRC,
      mappedAtCreation: true,
    });
    staging.push({
      buf,
      arrayBuf: new Float32Array(buf.getMappedRange()),
    });
  }
  let frameIndex = 0;

  // Worker pool
  const NUM_WORKERS = 4;
  const workers = Array.from(
    { length: NUM_WORKERS },
    () =>
      new Worker(new URL("./worker.js", import.meta.url), { type: "module" })
  );
  let rr = 0;
  for (const w of workers) {
    w.onmessage = ({ data: records }) => {
      for (const { ts, val } of records) {
        tsBuf[writeIdx % MAX] = ts;
        valBuf[writeIdx % MAX] = val;
        writeIdx++;
      }
      count = Math.min(count + records.length, MAX);
    };
  }

  // WebSocket → round-robin workers
  const socket = new WebSocket(`ws://${location.hostname}:3032/signal`);
  socket.binaryType = "arraybuffer";
  socket.onmessage = ({ data }) => {
    const w = workers[rr];
    w.postMessage(data, [data]);
    rr = (rr + 1) % NUM_WORKERS;
  };
  function updateTooltipAt(x, y) {
    // mirror the exact logic from your mousemove handler,
    // but using x,y instead of e.clientX/Y:

    const rect = canvas.getBoundingClientRect();
    const mx = (x - rect.left) / rect.width;
    const ageSec = (1 - mx) * 60;
    const targetNs = nowNs() - BigInt(Math.floor(ageSec * 1e9));

    // binary‐search & interpolate exactly as before:
    let lo = 0,
      hi = count - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      const idx = Number((writeIdx - count + mid + MAX) % MAX);
      if (tsBuf[idx] < targetNs) lo = mid + 1;
      else hi = mid;
    }
    const i1 = lo,
      i0 = Math.max(0, lo - 1);
    const j0 = Number((writeIdx - count + i0 + MAX) % MAX);
    const j1 = Number((writeIdx - count + i1 + MAX) % MAX);
    const t0 = tsBuf[j0],
      t1 = tsBuf[j1];
    const v0 = valBuf[j0],
      v1 = valBuf[j1];
    const frac =
      t1 === t0 ? 0 : Number((targetNs - t0) * 1n) / Number((t1 - t0) * 1n);
    const interpValue = v0 + (v1 - v0) * frac;
    const interpNs = t0 + BigInt(Math.floor(frac * Number(t1 - t0)));
    const msTotal = interpNs / 1_000_000n;
    const date = new Date(Number(msTotal));
    const remNs = interpNs % 1_000_000_000n;
    const usec = remNs / 1_000n;
    const nsec = remNs % 1_000n;
    const pad = (n, w) => n.toString().padStart(w, "0");
    const Y = date.getFullYear(),
      M = pad(date.getMonth() + 1, 2),
      D = pad(date.getDate(), 2),
      h = pad(date.getHours(), 2),
      m = pad(date.getMinutes(), 2),
      s = pad(date.getSeconds(), 2),
      fracPart = `${pad(usec, 6)}:${pad(nsec, 3)}`,
      timeStr = `${Y}/${M}/${D} ${h}:${m}:${s}.${fracPart}`;

    tooltip.style.display = "block";
    tooltip.innerHTML =
      `<div><strong>Value:</strong> ${interpValue.toFixed(2)}</div>` +
      `<div><strong>Time:</strong>  ${timeStr}</div>`;
    tooltip.style.left = `${x + 12}px`;
    tooltip.style.top = `${y + 12}px`;
  }

  // Tooltip
  canvas.addEventListener("mousemove", (e) => {
    lastMouseX = e.clientX;
    lastMouseY = e.clientY;
    if (count === 0) return;
    updateTooltipAt(e.clientX, e.clientY);
  });
  canvas.addEventListener("mouseleave", () => (tooltip.style.display = "none"));
  async function frame() {
    const now = nowNs();

    // ── 1) Evict samples older than 60s by advancing startIdx ──
    while (writeIdx > startIdx) {
      const age = Number(now - tsBuf[startIdx % MAX]) / 1e9;
      if (age > 60) {
        startIdx++;
      } else {
        break;
      }
    }

    // ── 2) Compute how many remain ──
    const currentCount = writeIdx - startIdx;
    if (currentCount === 0) {
      requestAnimationFrame(frame);
      return;
    }

    // ── 3) Autoscale & rebuild vertArr ──
    let vmin = Infinity,
      vmax = -Infinity;
    for (let i = 0; i < currentCount; i++) {
      const idx = (startIdx + i) % MAX;
      const v = valBuf[idx];
      vmin = Math.min(vmin, v);
      vmax = Math.max(vmax, v);
    }
    if (vmin === vmax) vmax = vmin + 1;
    for (let i = 0; i < currentCount; i++) {
      const idx = (startIdx + i) % MAX;
      const age = Number(now - tsBuf[idx]) / 1e9;
      vertArr[i * 2] = 1 - (age / 60) * 2;
      vertArr[i * 2 + 1] = ((valBuf[idx] - vmin) / (vmax - vmin)) * 2 - 1;
    }

    // ── 4) Prepare staging buffer ──
    const bufIdx = frameIndex % FRAMES_IN_FLIGHT;
    const { buf, arrayBuf } = staging[bufIdx];
    const sliceLen = currentCount * 2;

    // Copy packed verts into staging
    for (let i = 0; i < sliceLen; i++) {
      arrayBuf[i] = vertArr[i];
    }
    buf.unmap();

    // ── 5) GPU copy + draw ──
    const enc = device.createCommandEncoder();
    const copyBytes = sliceLen * 4;
    enc.copyBufferToBuffer(buf, 0, vbo, 0, copyBytes);

    const pass = enc.beginRenderPass({
      colorAttachments: [
        {
          view: context.getCurrentTexture().createView(),
          clearValue: { r: 0.1, g: 0.1, b: 0.1, a: 1 },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });

    // 1) grid
    pass.setPipeline(axisPipeline);
    pass.setVertexBuffer(0, gridBuffer);
    pass.draw(gridVerts.length / 2, 1, 0, 0);

    // 2) axes
    pass.setVertexBuffer(0, axisBuffer);
    pass.draw(4, 1, 0, 0);

    // 3) live data
    pass.setPipeline(pipeline);
    pass.setVertexBuffer(0, vbo);
    pass.draw(currentCount, 1, 0, 0);

    pass.end();
    device.queue.submit([enc.finish()]);

    // ── 6) Remap for next frame + schedule ──
    await buf.mapAsync(GPUMapMode.WRITE);
    staging[bufIdx].arrayBuf = new Float32Array(buf.getMappedRange());

    frameIndex++;
    if (tooltip.style.display !== "none") {
      updateTooltipAt(lastMouseX, lastMouseY);
    }
    requestAnimationFrame(frame);
  }

  // Kick it off
  requestAnimationFrame(frame);
}
initLiveGraph();
