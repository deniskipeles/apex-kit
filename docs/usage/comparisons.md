# Industry Comparisons: BaaS Landscape

How does ApexKit stack up against the giants? This guide provides an honest comparison with Firebase, Supabase, and PocketBase.

## ApexKit vs. PocketBase

PocketBase is the closest relative to ApexKit—both are single-binary Go/Rust solutions using SQLite.

| Feature | PocketBase | ApexKit |
| :--- | :--- | :--- |
| **Language** | Go | Rust |
| **AI Features** | None (Plugin based) | **Native** (Vector Search, LLM) |
| **Scripting** | JS (k6/goja) | JS (**Boa** - Safe Rust) |
| **Search** | SQLite FTS | **Tantivy** (High-perf Full-text) |
| **Multi-tenancy** | No | **Yes** (Built-in) |

**Why choose ApexKit?** If you need native AI capabilities, built-in multi-tenancy, or the extreme performance of Rust.
**Downsides:** PocketBase has a larger community and more third-party plugins today.

## ApexKit vs. Supabase

Supabase is the "Postgres" powerhouse. It's a suite of many services (PostgREST, GoTrue, Realtime, etc.).

| Feature | Supabase | ApexKit |
| :--- | :--- | :--- |
| **Architecture** | Microservices (Docker Compose) | **Single Binary** |
| **Database** | PostgreSQL | SQLite |
| **Scaling** | Vertical/Horizontal (Complex) | Vertical / **gRPC Replicas** |
| **Setup Time** | 5-10 minutes | **10 seconds** |

**Why choose ApexKit?** For edge deployments, simpler local development, and projects where the overhead of Postgres is unnecessary. ApexKit's AI integration is also more "all-in-one" than pgvector setup.
**Downsides:** Postgres is more mature for massive write-heavy relational workloads (multi-terabyte).

## ApexKit vs. Firebase

The veteran BaaS from Google. Closed-source and proprietary.

| Feature | Firebase | ApexKit |
| :--- | :--- | :--- |
| **Hosting** | Google Cloud | **Self-hosted / Any Cloud** |
| **Pricing** | Usage-based (can be expensive) | **Free / Flat Server Cost** |
| **Offline Support** | Excellent | Good (Client SDK) |
| **Vendor Lock-in** | High | **None** (It's just SQLite) |

**Why choose ApexKit?** To avoid vendor lock-in, control your data costs, and keep your data in a specific geographic region (sovereignty).
**Downsides:** Firebase has a massive ecosystem of mobile-specific features (Analytics, Crashlytics, AdMob) that ApexKit does not aim to replace.

## Summary: Where ApexKit Stands Out

1. **The AI Edge**: ApexKit is the only BaaS where Vector search and LLM prompts are first-class citizens.
2. **Developer Velocity**: One binary, zero dependencies. You can go from `git clone` to a production API in minutes.
3. **Rust Ecosystem**: Leveraging the safety and speed of the Rust language and libraries like Axum and Candle.

## Downsides and Limitations

- **Ecosystem Maturity**: ApexKit is newer. Expect fewer StackOverflow answers and community templates.
- **SQLite Concurrency**: While SQLite is fast, it's not designed for thousands of *simultaneous writers* like Postgres or DynamoDB. It's better for high-read, moderate-write workloads.
- **No Native Mobile SDKs**: Currently, the focus is on the JavaScript/TypeScript SDK. Flutter/Swift/Kotlin developers will need to use the REST API directly.
