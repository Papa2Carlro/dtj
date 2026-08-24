#include <dtj_cpp/trace_session.hpp>
#include <cassert>
#include <iostream>

int main() {
    std::cout << "Running C++ wrapper tests..." << std::endl;

    // Test 1: Open disabled session (no agent)
    {
        dtj::config cfg;
        cfg.producer_name = "test";
        cfg.producer_version = "1.0.0";
        cfg.enabled = false;
        
        dtj::trace_session trace = dtj::trace_session::open(cfg);
        assert(!trace.is_enabled());
        
        // Emit should succeed (no-op on disabled)
        dtj::event e;
        e.domain = "test";
        e.category = "cat";
        e.name = "event";
        e.severity = DTJ_SEVERITY_INFO;
        e.field_name = "field";
        trace.emit(e);
        
        std::cout << "Test 1 PASS: Disabled session works" << std::endl;
    }

    // Test 2: OpenStrict with disabled mode
    {
        dtj::config cfg;
        cfg.producer_name = "test";
        cfg.producer_version = "1.0.0";
        cfg.enabled = false;
        
        dtj::trace_session trace = dtj::trace_session::open_strict(cfg);
        assert(!trace.is_enabled());
        
        std::cout << "Test 2 PASS: OpenStrict disabled works" << std::endl;
    }

    // Test 3: Value construction
    {
        dtj::value v1(true);
        assert(v1.kind() == dtj::value::kind::boolean);
        
        dtj::value v2(int64_t(42));
        assert(v2.kind() == dtj::value::kind::i64);
        
        dtj::value v3(3.14);
        assert(v3.kind() == dtj::value::kind::f64);
        
        dtj::value v4(std::string("hello"));
        assert(v4.kind() == dtj::value::kind::string);
        
        uint8_t bytes[] = {1, 2, 3};
        dtj::value v5(bytes, 3);
        assert(v5.kind() == dtj::value::kind::bytes);
        
        std::cout << "Test 3 PASS: Value construction works" << std::endl;
    }

    // Test 4: Move semantics
    {
        dtj::config cfg;
        cfg.producer_name = "test";
        cfg.producer_version = "1.0.0";
        cfg.enabled = false;
        
        dtj::trace_session trace1 = dtj::trace_session::open(cfg);
        dtj::trace_session trace2 = std::move(trace1);
        
        assert(trace2.is_enabled() == false);
        
        std::cout << "Test 4 PASS: Move semantics work" << std::endl;
    }

    std::cout << "\nAll C++ wrapper tests passed!" << std::endl;
    return 0;
}
