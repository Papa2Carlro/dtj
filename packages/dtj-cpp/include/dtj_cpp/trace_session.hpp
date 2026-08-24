/**
 * dtj-cpp: C++ RAII wrapper for dtj-c
 * 
 * This header-only wrapper provides RAII semantics over the dtj-c C API.
 * Links against dtj-c library.
 */

#ifndef DTJ_CPP_TRACE_SESSION_HPP
#define DTJ_CPP_TRACE_SESSION_HPP

#include <dtj/dtj.h>
#include <stdexcept>
#include <string>
#include <utility>

namespace dtj {

/**
 * Exception thrown on strict mode errors or protocol failures.
 */
class dtj_error : public std::runtime_error {
public:
    explicit dtj_error(const std::string& msg) : std::runtime_error(msg) {}
    explicit dtj_error(const char* msg) : std::runtime_error(msg) {}
};

/**
 * Configuration for opening a trace session.
 */
struct config {
    std::string data_dir = "./traces";
    std::string producer_name;
    std::string producer_version;
    std::string agent_path;
    std::string socket_path;
    std::string session_file_name;
    bool enabled = true;

    void (*warning_handler)(const char* message, void* user_data) = nullptr;
    void* warning_user_data = nullptr;
};

/**
 * Value types for event emission (MVP: exactly one field per event).
 */
class value {
public:
    enum class kind { none, boolean, i64, f64, string, bytes };
    
    constexpr value() noexcept : kind_(kind::none), bool_val_(false), i64_val_(0), f64_val_(0.0), bytes_data_(nullptr), bytes_len_(0) {}
    
    constexpr value(bool v) noexcept : kind_(kind::boolean), bool_val_(v) {}
    
    template<typename T,
             typename = typename std::enable_if<std::is_integral<T>::value && !std::is_same<T, bool>::value>::type>
    constexpr value(T v) noexcept : kind_(kind::i64), i64_val_(static_cast<int64_t>(v)) {}
    
    template<typename T,
             typename = typename std::enable_if<std::is_floating_point<T>::value>::type>
    constexpr value(T v) noexcept : kind_(kind::f64), f64_val_(static_cast<double>(v)) {}
    
    value(const std::string& s) noexcept : kind_(kind::string), string_storage_(s) {}
    value(const char* s) noexcept : kind_(kind::string), string_storage_(s ? s : "") {}
    
    value(const uint8_t* data, uint32_t len) noexcept : kind_(kind::bytes), bytes_data_(data), bytes_len_(len) {}

    // Copy/move - caller manages lifetime of external pointers
    value(const value& other) = default;
    value& operator=(const value& other) = default;
    
    value(value&& other) noexcept 
        : kind_(other.kind_), string_storage_(std::move(other.string_storage_)),
          bytes_data_(other.bytes_data_), bytes_len_(other.bytes_len_) {
        other.kind_ = kind::none;
        other.bytes_data_ = nullptr;
        other.bytes_len_ = 0;
    }
    
    value& operator=(value&& other) noexcept {
        if (this != &other) {
            string_storage_ = std::move(other.string_storage_);
            bytes_data_ = other.bytes_data_;
            bytes_len_ = other.bytes_len_;
            kind_ = other.kind_;
            other.kind_ = kind::none;
        }
        return *this;
    }
    
    enum class kind { none, boolean, i64, f64, string, bytes } kind() const noexcept { return kind_; }
    
private:
    enum class kind { none, boolean, i64, f64, string, bytes } kind_ = kind::none;
    
    union {
        bool bool_val_;
        int64_t i64_val_;
        double f64_val_;
        struct { const uint8_t* data; uint32_t len; } bytes_val_;
        
        constexpr void set_bool(bool v) { bool_val_ = v; }
        constexpr void set_i64(int64_t v) { i64_val_ = v; }
        constexpr void set_f64(double v) { f64_val_ = v; }
        
        constexpr bool get_bool() const { return bool_val_; }
        constexpr int64_t get_i64() const { return i64_val_; }
        constexpr double get_f64() const { return f64_val_; }
        
        bool bool_val_;
        int64_t i64_val_;
        double f64_val_;
    } storage_;
    
    std::string string_storage_;
    const uint8_t* bytes_data_ = nullptr;
    uint32_t bytes_len_ = 0;

public:
    friend class trace_session;
};

/**
 * Event structure for emitting trace events (MVP: exactly one field).
 */
struct event {
private:
friend class trace_session;

struct ValueHolder {
enum class Kind { none, boolean, i64, f64, string, bytes } k;

union Storage {
bool b;
int ival; long lval; long long llval; intptr_t iaddr;
float fval; double dval;
const char* cstr;
struct Bytes { const uint8_t* data; uint32_t len; } bytes;

Storage() : b(false), ival(0), lval(0), llval(0), iaddr(0), fval(0.0f), dval(0.0), cstr(nullptr) {}
} u;

Kind k;

ValueHolder() : k(Kind_none) {}

ValueHolder(bool v): k(Kind_bool) { u.b=v; }

template<typename T,
typename std::enable_if<std::is_same<T,int>::value ||
std::is_same<T,long>::value ||
std::is_same<T,long long>::value ||
std::is_same<T,int>::value>::type* = nullptr>
ValueHolder(T v): k(Kind_i64) { u.i=v; u.l=(long)(v); u.ll=(long long)(v); u.iaddr=(intptr_t)(v); }

template<typename T,
typename std::
};

private:
enum Kind_none {};
enum Kind_bool {};
enum Kind_i64 {};
enum Kind_f64 {};
enum Kind_string {};
enum Kind_bytes {};

union {
bool bval; int ival; long lval; long long llval; intptr_t iaddr;
float fval; double dval;
const char* cstr;
struct Bytes { const uint8_t* data; uint32_t len; } bytes;

Storage() : bval(false), ival(0), lval(0), llval(0), iaddr(0), fval(0.0f), dval(0.0), cstr(nullptr) {}
} u_;

friend class trace_session;

public:
event() {}

event(const event&)=default;
event& operator=(const event&)=default;

public:
std::string domain;
std::string category;
std::string name;
dtj_severity severity = DTJ_SEVERITY_INFO;
std::string field_name;

template<typename T>
event(const std::string& domain_, const std::string& category_, const std::string& name_, dtj_severity severity_, const std::string& field_name_, T value)
: domain(domain_), category(category_), name(name_), severity(severity_), field_name(field_name_) {}

template<typename T,
typename std::
};

private:
struct trace_session;

namespace detail {
class ValueConverter {
public:
static dtj_value convert(const event& e);
};

}

class trace_session {
private:
dtj_session* session_ = nullptr;

trace_session(dtj_session* sess=nullptr);

public:

~trace_session();
trace_session(trace_session&& other);
trace_session& operator=(trace_session&& other);

trace_session(const trace_session&)=delete;
trace_session& operator=(const trace_session&)=delete;

[[nodiscard]]bool is_enabled()const noexcept;
void emit(const event& ev);

static trace_session open(const config& cfg);
static trace_session open_strict(const config& cfg);

};

// C++ usage example:
// #include <dtj_cpp/trace_session.hpp>
// int main() {
// try {
// auto trace = dtj_cpp_trace_session_open(dtj_cpp_config{"my-service","1.0"});
// if(trace.is_enabled()){
// trace.emit({"api","request","completed",DTJ_SEVERITY_INFO,"duration_ms",12.5});
// }
// } catch(const dtj_error& e){...}
// }

} // namespace dtj

#endif // DTJ_CPP_TRACE_SESSION_HPP
