# Codecov Setup Guide

This document explains how to set up Codecov for the Coapum project.

## Prerequisites

1. Create a Codecov account at https://codecov.io
2. Add your GitHub repository to Codecov

## Setting up the CODECOV_TOKEN Secret

To enable code coverage reporting, you need to configure the `CODECOV_TOKEN` repository secret in GitHub:

1. Go to your repository on GitHub
2. Navigate to Settings > Secrets and variables > Actions
3. Click "New repository secret"
4. Set the name to `CODECOV_TOKEN`
5. Copy the token from your Codecov repository settings and paste it as the value
6. Click "Add secret"

## How Codecov Works

The Codecov integration is configured in `.github/workflows/ci.yml`. The coverage job:

1. Runs only on pushes to the master branch (not on pull requests)
2. Generates code coverage data using `grcov`
3. Uploads the coverage report to Codecov

## Troubleshooting

If the Codecov action fails with "Token required - not valid tokenless upload":

1. Verify that the `CODECOV_TOKEN` secret is set correctly
2. Check that the token hasn't expired
3. Ensure the repository is properly configured in Codecov

If you see "Unexpected input(s) 'file'" errors:

1. This has been fixed in the current configuration
2. The parameter should be `files` not `file`