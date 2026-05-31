{% if allocator == "jemalloc" %}// jemalloc reduces fragmentation and allocator contention under the multi-threaded
// runtime. It does not build under MSVC.
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
{% elif allocator == "mimalloc" %}#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;
{% endif %}