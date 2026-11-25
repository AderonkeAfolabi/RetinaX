# AI provider reputation

Each first administrator verification contributes one sample, 10,000 basis points for acceptance and zero for rejection. The first sample initializes the score. Later samples average with the decayed score. Reads halve the score for every complete 30-day period since its last sample. A provider with no samples has no score. Records created before this change start accumulating reputation with new reviews.
