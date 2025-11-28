#!/bin/bash

# LuaJIT-Pro Test Runner
# This script runs all Lua/Teal test cases in the tests directory

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
LUAJIT_BIN="${PROJECT_ROOT}/luajit2.1/bin/luajit"
GOLDEN_DIR="${SCRIPT_DIR}/golden"
BUILD_CACHE_DIR="${PROJECT_ROOT}/.luajit_pro/build_cache"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
PASSED=0
FAILED=0
SKIPPED=0

# Failed tests list
FAILED_TESTS=()

# Function to print colored output
print_header() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_failure() {
    echo -e "${RED}✗ $1${NC}"
}

print_skip() {
    echo -e "${YELLOW}○ $1 (skipped)${NC}"
}

# Check if LuaJIT binary exists
check_luajit() {
    if [ ! -f "${LUAJIT_BIN}" ]; then
        echo -e "${RED}Error: LuaJIT binary not found at ${LUAJIT_BIN}${NC}"
        echo -e "${YELLOW}Please run 'bash build.sh' first to build luajit-pro${NC}"
        exit 1
    fi
}

# Run a single Lua/Teal test file
run_single_test() {
    local test_file="$1"
    local test_name=$(basename "${test_file}")
    
    echo -n "Running ${test_name}... "
    
    # Set LUA_PATH for require to work
    export LUA_PATH="${PROJECT_ROOT}/tests/?.lua;${PROJECT_ROOT}/?.lua;;"
    
    # Clean cache before each test
    rm -rf "${BUILD_CACHE_DIR}" 2>/dev/null || true
    
    # Run the test and capture output
    local output
    output=$("${LUAJIT_BIN}" "${test_file}" 2>&1)
    local exit_code=$?
    
    if [ ${exit_code} -eq 0 ]; then
        print_success "passed"
        ((PASSED++))
    else
        print_failure "failed"
        ((FAILED++))
        FAILED_TESTS+=("${test_name}")
        # Show error output for failed tests
        if [ -n "${output}" ]; then
            echo -e "${RED}  Error output:${NC}"
            echo "${output}" | head -20 | sed 's/^/    /'
            local lines=$(echo "${output}" | wc -l)
            if [ ${lines} -gt 20 ]; then
                echo "    ... (${lines} lines total, showing first 20)"
            fi
        fi
    fi
}

# Run a golden file test - compares transformed output against expected output
run_golden_test() {
    local test_file="$1"
    local test_name=$(basename "${test_file}")
    local expected_file="${test_file%.lua}.expected.lua"
    local test_dir=$(dirname "${test_file}")
    local local_cache_dir="${test_dir}/.luajit_pro/build_cache"
    
    echo -n "Running golden ${test_name}... "
    
    # Check if expected file exists
    if [ ! -f "${expected_file}" ]; then
        print_skip "no expected file"
        ((SKIPPED++))
        return
    fi
    
    # Set LUA_PATH for require to work (include golden directory for helper files)
    export LUA_PATH="${GOLDEN_DIR}/?.lua;${PROJECT_ROOT}/tests/?.lua;${PROJECT_ROOT}/?.lua;;"
    
    # Clean cache before test (in both locations)
    rm -rf "${BUILD_CACHE_DIR}" 2>/dev/null || true
    rm -rf "${local_cache_dir}" 2>/dev/null || true
    
    # Change to test directory so cache is created there
    cd "${test_dir}"
    
    # Run the test and capture output
    local output
    output=$("${LUAJIT_BIN}" "${test_file}" 2>&1)
    local exit_code=$?
    
    if [ ${exit_code} -ne 0 ]; then
        print_failure "execution failed"
        ((FAILED++))
        FAILED_TESTS+=("golden:${test_name}")
        if [ -n "${output}" ]; then
            echo -e "${RED}  Error output:${NC}"
            echo "${output}" | head -10 | sed 's/^/    /'
        fi
        return
    fi
    
    # Get the generated file from local cache (relative to test directory)
    local generated_file="${local_cache_dir}/${test_name}"
    
    if [ ! -f "${generated_file}" ]; then
        print_failure "no output generated"
        ((FAILED++))
        FAILED_TESTS+=("golden:${test_name}")
        return
    fi
    
    # Compare with expected output
    if diff -q "${generated_file}" "${expected_file}" > /dev/null 2>&1; then
        print_success "passed"
        ((PASSED++))
    else
        print_failure "output mismatch"
        ((FAILED++))
        FAILED_TESTS+=("golden:${test_name}")
        echo -e "${RED}  Diff (expected vs actual):${NC}"
        diff "${expected_file}" "${generated_file}" | head -20 | sed 's/^/    /'
    fi
}

# Run all Lua tests
run_lua_tests() {
    print_header "Running Lua Tests"
    
    cd "${PROJECT_ROOT}/tests"
    
    # Run .lua test files
    for test_file in test_*.lua; do
        if [ -f "${test_file}" ]; then
            run_single_test "${SCRIPT_DIR}/${test_file}"
        fi
    done
}

# Run golden file tests
run_golden_tests() {
    print_header "Running Golden File Tests"
    
    if [ ! -d "${GOLDEN_DIR}" ]; then
        echo "No golden directory found, skipping"
        return
    fi
    
    cd "${GOLDEN_DIR}"
    
    # Run golden .lua test files (skip .expected.lua files)
    for test_file in test_golden_*.lua; do
        if [ -f "${test_file}" ] && [[ ! "${test_file}" == *.expected.lua ]]; then
            run_golden_test "${GOLDEN_DIR}/${test_file}"
        fi
    done
}

# Run Teal tests (skip by default as tl module might not be available)
run_teal_tests() {
    print_header "Running Teal Tests"
    
    cd "${PROJECT_ROOT}/tests"
    
    # Check if there are any .tl test files
    local tl_files=(test_*.tl)
    if [ ! -f "${tl_files[0]}" ]; then
        echo "No Teal test files found"
        return
    fi
    
    # Run .tl test files
    for test_file in test_*.tl; do
        if [ -f "${test_file}" ]; then
            run_single_test "${SCRIPT_DIR}/${test_file}"
        fi
    done
}

# Print summary
print_summary() {
    print_header "Test Summary"
    
    local TOTAL=$((PASSED + FAILED + SKIPPED))
    
    echo -e "Total:   ${TOTAL}"
    echo -e "${GREEN}Passed:  ${PASSED}${NC}"
    echo -e "${RED}Failed:  ${FAILED}${NC}"
    echo -e "${YELLOW}Skipped: ${SKIPPED}${NC}"
    
    if [ ${FAILED} -gt 0 ]; then
        echo ""
        echo -e "${RED}Failed tests:${NC}"
        for test in "${FAILED_TESTS[@]}"; do
            echo -e "  ${RED}• ${test}${NC}"
        done
        echo ""
        echo -e "${RED}Some tests failed!${NC}"
        return 1
    else
        echo ""
        echo -e "${GREEN}All tests passed!${NC}"
        return 0
    fi
}

# Update golden files - regenerate expected output files
update_golden() {
    print_header "Updating Golden Files"
    
    if [ ! -d "${GOLDEN_DIR}" ]; then
        echo "No golden directory found"
        return 1
    fi
    
    cd "${GOLDEN_DIR}"
    local local_cache_dir="${GOLDEN_DIR}/.luajit_pro/build_cache"
    
    for test_file in test_golden_*.lua; do
        if [ -f "${test_file}" ] && [[ ! "${test_file}" == *.expected.lua ]]; then
            local test_name=$(basename "${test_file}")
            local expected_file="${test_file%.lua}.expected.lua"
            
            echo -n "Updating ${test_name}... "
            
            # Clean cache (in golden directory)
            rm -rf "${local_cache_dir}" 2>/dev/null || true
            
            # Set LUA_PATH (include golden directory for helper files)
            export LUA_PATH="${GOLDEN_DIR}/?.lua;${PROJECT_ROOT}/tests/?.lua;${PROJECT_ROOT}/?.lua;;"
            
            # Run the test and wait for it to complete
            local output
            output=$("${LUAJIT_BIN}" "${GOLDEN_DIR}/${test_file}" 2>&1)
            local exit_code=$?
            
            if [ ${exit_code} -eq 0 ]; then
                # Copy generated file to expected
                local generated_file="${local_cache_dir}/${test_name}"
                if [ -f "${generated_file}" ]; then
                    cp "${generated_file}" "${expected_file}"
                    print_success "updated"
                else
                    print_failure "no output file"
                    echo "  Expected file at: ${generated_file}"
                    echo "  Cache dir contents: $(ls -la "${local_cache_dir}" 2>&1)"
                fi
            else
                print_failure "execution failed"
                echo "  ${output}" | head -5
            fi
        fi
    done
}

# Main execution
main() {
    print_header "LuaJIT-Pro Test Suite"
    
    echo "Project root: ${PROJECT_ROOT}"
    echo "LuaJIT binary: ${LUAJIT_BIN}"
    echo ""
    
    # Check prerequisites
    check_luajit
    
    # Run Lua tests
    run_lua_tests
    echo ""
    
    # Run golden file tests
    run_golden_tests
    echo ""
    
    # Print summary and exit with appropriate code
    print_summary
}

# Parse command line arguments
case "${1:-all}" in
    lua)
        check_luajit
        run_lua_tests
        print_summary
        ;;
    golden)
        check_luajit
        run_golden_tests
        print_summary
        ;;
    teal)
        check_luajit
        run_teal_tests
        print_summary
        ;;
    update-golden)
        check_luajit
        update_golden
        ;;
    all|*)
        main
        ;;
esac
