#include "dtj/dtj.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <limits.h>
#include <signal.h>
#include <time.h>

#define DTJ_TEMP_SOCKET_PREFIX "dtj-agent-"
#define DTJ_STARTUP_TIMEOUT_MS 5000
#define DTJ_RETRY_INTERVAL_MS 10

typedef struct {
    pid_t pid;
    char temp_dir[PATH_MAX];
    char socket_path[PATH_MAX];
    int warning_emitted;
} dtj_discovery;

/* Find dtj-agent binary using discovery order */
static int dtj_find_agent(const char *agent_path, char *out_path, size_t out_path_size) {
    if (agent_path && *agent_path) {
        if (access(agent_path, X_OK) == 0) {
            strncpy(out_path, agent_path, out_path_size - 1);
            out_path[out_path_size - 1] = '\0';
            return DTJ_OK;
        }
        return DTJ_ERROR_AGENT_NOT_FOUND;
    }

    const char *env_path = getenv("DTJ_AGENT_PATH");
    if (env_path && *env_path) {
        if (access(env_path, X_OK) == 0) {
            strncpy(out_path, env_path, out_path_size - 1);
            out_path[out_path_size - 1] = '\0';
            return DTJ_OK;
        }
    }

    /* PATH lookup - simplified, assumes dtj-agent is in PATH */
    if (access("dtj-agent", X_OK) == 0) {
        strncpy(out_path, "dtj-agent", out_path_size - 1);
        out_path[out_path_size - 1] = '\0';
        return DTJ_OK;
    }

    return DTJ_ERROR_AGENT_NOT_FOUND;
}

/* Create temporary directory for socket */
static int dtj_create_temp_socket_dir(char *out_dir, size_t out_dir_size) {
    char template[PATH_MAX];
    snprintf(template, sizeof(template), "/tmp/%sXXXXXX", DTJ_TEMP_SOCKET_PREFIX);
    
    char *result = mkdtemp(template);
    if (!result) return -1;
    
    strncpy(out_dir, template, out_dir_size - 1);
    out_dir[out_dir_size - 1] = '\0';
    return DTJ_OK;
}

/* Wait for socket to become available */
static int dtj_wait_for_socket(const char *socket_path, int timeout_ms) {
    int elapsed = 0;
    while (elapsed < timeout_ms) {
        if (access(socket_path, F_OK) == 0) {
            /* Try to connect to verify it's ready */
            return DTJ_OK;
        }
        usleep(DTJ_RETRY_INTERVAL_MS * 1000);
        elapsed += DTJ_RETRY_INTERVAL_MS;
    }
    return -1;
}

/* Spawn agent process */
static pid_t dtj_spawn_agent(const char *agent_binary,
                              const char *socket_path,
                              const char *data_dir) {
    pid_t pid = fork();
    if (pid == 0) {
        /* Child process */
        execl(agent_binary, "dtj-agent", "--socket", socket_path, "--data-dir", data_dir, (char*)NULL);
        _exit(127); /* exec failed */
    }
    return pid; /* parent gets child PID */
}

/* Initialize discovery and start agent */
int dtj_discovery_start(const dtj_config *config,
                         const char *session_file_name,
                         dtj_discovery **out_discovery,
                         char *out_socket_path,
                         size_t socket_path_size,
                         dtj_error *out_error) {
    
    if (!config || !config->producer_name || !config->producer_version) {
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_VALUE, "Missing required config fields");
        return DTJ_ERROR_VALUE;
    }

    /* Find agent binary */
    char agent_binary[PATH_MAX];
    int find_result = dtj_find_agent(config->agent_path, agent_binary, sizeof(agent_binary));
    
    /* If socket_path provided, don't start agent - connect to existing */
    if (config->socket_path && *config->socket_path) {
        dtj_discovery *disc = calloc(1, sizeof(dtj_discovery));
        if (!disc) { if (out_error) dtj_error_init(out_error, DTJ_ERROR_VALUE, "OOM"); return DTJ_ERROR_VALUE; }
        strncpy(disc->socket_path, config->socket_path, sizeof(disc->socket_path) - 1);
        *out_discovery = disc;
        strncpy(out_socket_path, config->socket_path, socket_path_size - 1);
        out_socket_path[socket_path_size - 1] = '\0';
        return DTJ_OK;
    }

    /* Agent not found and no explicit socket path */
    if (find_result != DTJ_OK) {
        if (config->warning_handler && !config->enabled) { /* Actually warning on missing agent */
            config->warning_handler("dtj-agent not found. Install dtj-agent or set DTJ_AGENT_PATH. Tracing disabled.", config->warning_user_data);
        }
        return DTJ_ERROR_AGENT_NOT_FOUND;
    }

    /* Create temp directory for socket */
    dtj_discovery *disc = calloc(1, sizeof(dtj_discovery));
    if (!disc) { if (out_error) dtj_error_init(out_error, DTJ_ERROR_VALUE, "OOM"); return DTJ_ERROR_VALUE; }

    if (dtj_create_temp_socket_dir(disc->temp_dir, sizeof(disc->temp_dir)) != DTJ_OK) {
        free(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_CONNECTION, "Failed to create temp dir");
        return DTJ_ERROR_CONNECTION;
    }

    snprintf(disc->socket_path, sizeof(disc->socket_path), "%s/agent.sock", disc->temp_dir);

    /* Ensure data directory exists */
    const char *data_dir = config->data_dir ? config->data_dir : "./traces";
    mkdir(data_dir, 0755);

    /* Spawn agent */
    disc->pid = dtj_spawn_agent(agent_binary, disc->socket_path, data_dir);
    if (disc->pid <= 0) {
        free(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_CONNECTION, "Failed to spawn agent");
        return DTJ_ERROR_CONNECTION;
    }

    /* Wait for socket to be ready */
    if (dtj_wait_for_socket(disc->socket_path, DTJ_STARTUP_TIMEOUT_MS) != DTJ_OK) {
        kill(disc->pid, SIGTERM);
        waitpid(disc->pid, NULL, 0);
        free(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_CONNECTION, "Agent startup timeout");
        return DTJ_ERROR_CONNECTION;
    }

    *out_discovery = disc;
    strncpy(out_socket_path, disc->socket_path, socket_path_size - 1);
    out_socket_path[socket_path_size - 1] = '\0';
    
    return DTJ_OK;
}

/* Stop agent and cleanup */
void dtj_discovery_stop(dtj_discovery *disc) {
    if (!disc) return;

    if (disc->pid > 0) {
        kill(disc->pid, SIGTERM);
        int status;
        int waited = waitpid(disc->pid, &status, WNOHANG);
        if (waited == 0) {
            /* Wait up to 5 seconds */
            struct timespec ts = { .tv_sec = 5 };
            nanosleep(&ts, NULL);
            waitpid(disc->pid, &status, WNOHANG);
            kill(disc->pid, SIGKILL); /* Force kill if still alive */
            waitpid(disc->pid, &status, 0);
        }
        disc->pid = 0;
    }

    /* Cleanup temp directory */
    if (disc->temp_dir[0]) {
        char cmd[PATH_MAX + 32];
        snprintf(cmd, sizeof(cmd), "rm -rf %s", disc->temp_dir);
        system(cmd); /* Simple cleanup; in production use rmdir recursive */
        disc->temp_dir[0] = '\0';
    }
    
    free(disc);
}
