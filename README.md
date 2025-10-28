# Starrlight

> ⚠️ **Note**: This project is no longer being actively maintained or developed.

## Overview

Starrlight is a command-line tool written in Rust that helps you effortlessly create an Awesome List from your GitHub stars. It fetches your starred repositories using the GitHub GraphQL API and generates organized documentation categorized by programming language or topic.

## Features

- Fetch starred repositories using GitHub GraphQL API
- Categorize repositories by programming language or topic
- Generate Awesome List formatted markdown files
- Console output for quick viewing
- Smart caching system to avoid GitHub API rate limits
- Memory-efficient streaming mode for processing large star lists
- Support for private repositories
- Configurable topic filtering based on stargazer count
- File splitting for large outputs to keep files manageable

## Building from Source

### Prerequisites

- Rust 1.88 or later
- A GitHub Personal Access Token with `repo` scope

### Build Steps

1. Clone the repository:
```bash
git clone https://github.com/yehezkieldio/starrlight-rust.git
cd starrlight-rust
```

2. Build the project:
```bash
cargo build --release
```

3. The binary will be available at `target/release/starrlight`

### Running

Set your GitHub token as an environment variable:
```bash
export GITHUB_TOKEN=your_github_token_here
```

Run the tool:
```bash
# Generate markdown output
./target/release/starrlight --username your_github_username --output markdown

# Console output
./target/release/starrlight --username your_github_username --output console

# Categorize by topic instead of language
./target/release/starrlight --username your_github_username --topic --output markdown

# Include private repositories
./target/release/starrlight --username your_github_username --private --output markdown
```

### Additional Options

```bash
# View cache statistics
./target/release/starrlight --username your_username cache stats

# Clear cache for a specific user
./target/release/starrlight cache clear --username your_username

# Clear all cache
./target/release/starrlight cache clear-all

# Force refresh (bypass cache)
./target/release/starrlight --username your_username --refresh --output markdown

# Customize output directory
./target/release/starrlight --username your_username --output markdown --output-dir ./my-stars
```

## License

MIT License

Copyright (c) 2025 Yehezkiel Dio Sinolungan

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
