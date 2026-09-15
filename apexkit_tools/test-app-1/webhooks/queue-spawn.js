export default async function (req) {
    const startTime = Date.now();
    const body = await req.json();

    // Default to 15,000,000 iterations of heavy mathematical hashing
    const iterations = body.iterations || 15_000_000;

    // Spawn the background queue job
    const job = await $queue.spawn(async (pid, jobReq) => {
        const payload = await jobReq.json();
        const target = payload.iterations || 10_000_000;
        
        let accumulator = 0;
        const computeStart = Date.now();

        // Heavy CPU-bound crunching loop
        for (let i = 0; i < target; i++) {
            accumulator = (accumulator + Math.sqrt(i) * 1.00001) % 1000000;
        }

        return {
            pid: pid,
            target_iterations: target,
            final_accumulator: accumulator,
            wall_time_ms: Date.now() - computeStart,
            completed_at: new Date().toISOString()
        };
    }, {
        // Allocate up to 5 minutes of execution time
        timeoutMs: 300_000, 
        args: { iterations: iterations }
    });

    // Return within milliseconds to the caller
    return new Response({
        message: "Background job successfully offloaded.",
        job_id: job.pid,
        initial_status: job.status,
        webhook_overhead_ms: Date.now() - startTime
    }, {
        status: 202,
        headers: { "Content-Type": "application/json" }
    });
}