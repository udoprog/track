-- Moves the Cloudflare Access settings from track's five cloudflare_* config
-- keys onto watchword's `cloudflare` key, the JSON that Auth::set_cloudflare
-- stores and Auth::cloudflare reads, which then holds them alone. Settings
-- watchword already stored are kept as they are: the server has run with them
-- since they were saved, as stored settings win over the ones track passed
-- it. Each key reads as track read it: missing booleans are off, except
-- verify_jwt, which is on unless stored as anything but 'true'.
INSERT
OR IGNORE INTO config (key, value)
SELECT
    'cloudflare',
    json_object(
        'enabled',
        json(
            CASE
                WHEN (
                    SELECT value FROM config WHERE key = 'cloudflare_access_enabled'
                ) = 'true' THEN 'true'
                ELSE 'false'
            END
        ),
        'team_domain',
        COALESCE(
            (SELECT value FROM config WHERE key = 'cloudflare_team_domain'),
            ''
        ),
        'audience',
        COALESCE(
            (SELECT value FROM config WHERE key = 'cloudflare_audience'),
            ''
        ),
        'trust_email_header',
        json(
            CASE
                WHEN (
                    SELECT value FROM config WHERE key = 'cloudflare_trust_email_header'
                ) = 'true' THEN 'true'
                ELSE 'false'
            END
        ),
        'verify_jwt',
        json(
            CASE
                WHEN COALESCE(
                    (SELECT value FROM config WHERE key = 'cloudflare_verify_jwt'),
                    'true'
                ) = 'true' THEN 'true'
                ELSE 'false'
            END
        )
    )
WHERE
    EXISTS (
        SELECT 1 FROM config
        WHERE key IN (
            'cloudflare_access_enabled',
            'cloudflare_team_domain',
            'cloudflare_audience',
            'cloudflare_trust_email_header',
            'cloudflare_verify_jwt'
        )
    );

DELETE FROM config
WHERE
    key IN (
        'cloudflare_access_enabled',
        'cloudflare_team_domain',
        'cloudflare_audience',
        'cloudflare_trust_email_header',
        'cloudflare_verify_jwt'
    );
