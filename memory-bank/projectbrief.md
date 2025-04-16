# Argus Project Brief

## Project Overview
Argus is an intelligent news monitoring system that processes, analyzes, and delivers relevant news content to users. The system fetches articles from RSS feeds, evaluates their relevance based on predefined topics, and provides detailed analysis of the content including its credibility, factual accuracy, and contextual significance.

## Core Functionality
1. **RSS Fetching & Processing**: Automatically gather articles from various news sources
2. **Content Analysis**: Evaluate articles for topic relevance and quality
3. **Threat Detection**: Identify and prioritize life safety threats
4. **Notification System**: Alert users about relevant content through Slack and a mobile application
5. **Smart Matching**: Connect related articles through vector similarity and entity relationships

## Key Requirements
- **Scalable Architecture**: Handle increasing numbers of sources and content volume
- **Analysis Accuracy**: Provide reliable and relevant content evaluations
- **Responsive Notifications**: Deliver time-sensitive information quickly
- **Contextual Understanding**: Recognize relationships between different news items
- **Entity-Based Matching**: Identify and track named entities (people, organizations, locations, events) across articles

## Target Users
- Information analysts
- Research professionals
- Decision-makers requiring timely news insights
- Safety and security monitoring teams

## Success Metrics
- Relevant article identification accuracy
- Time from publication to notification
- Quality of article analysis
- Successful clustering of related content
- Accurate entity extraction and relationship mapping

## Technical Foundation
- Rust-based backend for performance and reliability
- SQLite database for persistent storage
- Vector embeddings for semantic content matching
- Entity-based relationships for improved article matching
- LLM integration for content analysis
- Multi-worker architecture for concurrent processing
