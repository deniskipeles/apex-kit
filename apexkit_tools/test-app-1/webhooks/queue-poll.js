export default async function (req) {
    const url = new URL(req.url);
    const body = await req.json().catch(() => ({}));
    const pid = body.pid || url.searchParams.get("pid");

    if (!pid) {
        return new Response({ error: "Missing 'pid' parameter." }, { status: 400 });
    }

    const status = await $queue.status(pid);
    let result = null;

    // If terminal state, harvest the result
    if (["completed", "failed", "timed_out"].includes(status.status)) {
        result = await $queue.result(pid);
    }

    return new Response({
        pid: pid,
        status: status.status,
        runtime_ms: status.runtime_ms,
        error: status.error,
        data: result ? result.result : null
    }, {
        status: 200,
        headers: { "Content-Type": "application/json" }
    });
}