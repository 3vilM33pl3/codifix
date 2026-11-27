SELECT json_build_object('incident_id', :'incident'::uuid, 'conversation', :'chat'::uuid,
                         'received_recovery', EXISTS (
                           SELECT 1 FROM inbox_messages WHERE conversation_id=:'chat'::uuid
                             AND role='system' AND starts_with(body, 'Incident ' || :'incident' || ' · codifix-incident-canary · recovered' || chr(10))
                         ));
