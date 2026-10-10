#pragma once
#include <filesystem>
// Initialized native UI thread only. Empty result means cancelled/failed.
std::filesystem::path foundryChooseDirectory(void* parent,const char* title,const char* initial);
