ALTER TABLE orchestrator_settings ADD COLUMN dispatch_defaults TEXT NOT NULL
    DEFAULT '{"default_agent":null,"providers":{}}';
