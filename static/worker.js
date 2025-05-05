// worker.js — no exports needed if using native ES-module workers
self.onmessage = ({ data }) => {
  const dv = new DataView(data);
  const recs = Math.floor(dv.byteLength / 12);
  const out = new Array(recs);
  for (let i = 0; i < recs; i++) {
    const b = i * 12;
    out[i] = {
      ts: dv.getBigInt64(b, true),
      val: dv.getUint32(b + 8, true),
    };
  }
  self.postMessage(out);
};
