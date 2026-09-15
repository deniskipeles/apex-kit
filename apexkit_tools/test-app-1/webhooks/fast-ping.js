export default async function (req) {
    return new Response({
        pong: true,
        server_timestamp: Date.now()
    }, {
        status: 200,
        headers: { "Content-Type": "application/json" }
    });
}