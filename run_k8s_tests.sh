#!/bin/bash

# Check if cargo is installed
if ! command -v cargo &> /dev/null
then
    echo "cargo could not be found"
    exit
fi

# Check is project root directory
if [ ! -f Cargo.toml ]; then
    echo "Please run this script from the project root directory"
    exit
fi

build_project() {
    echo "Building project"
    cargo build --release
}

create_dummy_dir_if_not_exists() {
    if [ ! -d $1 ]; then
        mkdir -p $1
    fi
}

read_expected_ret_code() {
    file_path=$1/.expected_ret_code
    if [ -f $file_path ]; then
        excepted_ret_code=$(cat $file_path)
    fi

    echo $excepted_ret_code
}

to_absolute_path() {
    echo "$(cd "$(dirname "$1")"; pwd)/$(basename "$1")"
}

target_dir=sample/k8s/
deployfix_bin=target/release/deployfix-cli

# list directories in target_dir
test_cases=$(find $target_dir -maxdepth 1 -type d | sort)
# remove first line
test_cases=$(echo "$test_cases" | sed '1d')
total_test_cases=$(echo "$test_cases" | wc -l)

build_project
create_dummy_dir_if_not_exists dummy

echo "Running tests for the following directories: $total_test_cases Cases in total"
i=1
success=0
failure=0
for test_case in $test_cases; do
    printf "  Running $test_case ($i/$total_test_cases)"
    i=$((i+1))

    abs_test_case=$(to_absolute_path $test_case)
    abs_dummy_dir=$(to_absolute_path dummy)
    abs_output_dir=$(to_absolute_path output)

    $deployfix_bin k8s go $abs_test_case $abs_dummy_dir $abs_output_dir --cycle-check > /dev/null 2>&1
    ret_code=$?

    excepted_ret_code=$(read_expected_ret_code $test_case)

    if [ $ret_code -eq $excepted_ret_code ]; then
        echo -e "\e[32m    Test passed\e[0m"
        success=$((success+1))
    else
        echo -e "\e[31m    Test failed\e[0m"
        echo "    Expected return code: $excepted_ret_code"
        echo "    Actual return code: $ret_code"
        failure=$((failure+1))
    fi
done

echo "Test result: $success/$total_test_cases passed, $failure/$total_test_cases failed"